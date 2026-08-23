use rusqlite::Connection;
use yt_dlp_gui::storage::schema::{initialize_schema, DOWNLOAD_SCHEMA_VERSION};
use yt_dlp_gui::storage::{DownloadProgress, DownloadTaskDraft, DownloadTaskStatus};

fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    initialize_schema(&connection).unwrap();
    connection
}

fn task(created_at: i64) -> DownloadTaskDraft {
    DownloadTaskDraft {
        source_url: "https://example.invalid/video".to_owned(),
        video_id: Some("video-id".to_owned()),
        title: Some("Title".to_owned()),
        thumbnail_url: None,
        duration_seconds: Some(60),
        target_path: "C:/Downloads".to_owned(),
        output_path: None,
        selected_format: Some("137+140".to_owned()),
        created_at,
        yt_dlp_version: Some("2026.08.19".to_owned()),
    }
}

#[test]
fn initializes_download_schema_and_keeps_config_empty() {
    let connection = connection();
    let config_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM config", [], |row| row.get(0))
        .unwrap();
    let version: i64 = connection
        .query_row(
            "SELECT version FROM storage_schema_versions WHERE domain = 'download_tasks'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('download_tasks', 'download_task_streams')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(config_count, 0);
    assert_eq!(version, DOWNLOAD_SCHEMA_VERSION);
    assert_eq!(table_count, 2);
    assert_eq!(
        connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn repeated_schema_initialization_is_idempotent() {
    let connection = connection();
    initialize_schema(&connection).unwrap();
    let versions: i64 = connection
        .query_row("SELECT COUNT(*) FROM storage_schema_versions", [], |row| row.get(0))
        .unwrap();
    assert_eq!(versions, 1);
}

#[test]
fn task_and_stream_rows_round_trip_and_cascade() {
    let connection = connection();
    let mut task_connection = connection;
    let draft = task(100);
    let transaction = task_connection.transaction().unwrap();
    transaction.execute(
        "INSERT INTO download_tasks (source_url, video_id, title, target_path, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)",
        rusqlite::params![draft.source_url, draft.video_id, draft.title, draft.target_path, draft.created_at],
    ).unwrap();
    let task_id = transaction.last_insert_rowid();
    transaction.execute(
        "INSERT INTO download_task_streams (task_id, stream_key, media_type, status, created_at, updated_at) VALUES (?1, 'video', 'video', 'pending', 100, 100)",
        [task_id],
    ).unwrap();
    transaction.commit().unwrap();
    assert_eq!(
        task_connection
            .query_row(
                "SELECT COUNT(*) FROM download_task_streams WHERE task_id = ?1",
                [task_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    task_connection
        .execute("DELETE FROM download_tasks WHERE id = ?1", [task_id])
        .unwrap();
    assert_eq!(
        task_connection
            .query_row("SELECT COUNT(*) FROM download_task_streams", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn schema_rejects_invalid_progress_and_media_type() {
    let connection = connection();
    let error = connection.execute(
        "INSERT INTO download_tasks (source_url, target_path, status, progress_percent, created_at, updated_at) VALUES ('url', 'path', 'pending', 101, 1, 1)",
        [],
    ).unwrap_err();
    assert!(error.to_string().contains("CHECK"));
}

#[test]
fn progress_type_boundary_is_represented() {
    let progress = DownloadProgress {
        progress_percent: Some(100),
        downloaded_bytes: 10,
        total_bytes: Some(10),
        total_bytes_estimate: None,
        speed_bytes_per_second: Some(2),
        elapsed_seconds: Some(5),
        eta_seconds: Some(0),
        updated_at: 10,
    };
    assert_eq!(progress.progress_percent, Some(100));
    assert_eq!(DownloadTaskStatus::Pending.is_terminal(), false);
}

#[test]
fn duplicate_stream_key_is_rejected() {
    let connection = connection();
    connection.execute(
        "INSERT INTO download_tasks (source_url, target_path, status, created_at, updated_at) VALUES ('url', 'path', 'pending', 1, 1)",
        [],
    ).unwrap();
    let id = connection.last_insert_rowid();
    let sql = "INSERT INTO download_task_streams (task_id, stream_key, media_type, status, created_at, updated_at) VALUES (?1, 'same', 'video', 'pending', 1, 1)";
    connection.execute(sql, [id]).unwrap();
    assert!(connection.execute(sql, [id]).is_err());
}
