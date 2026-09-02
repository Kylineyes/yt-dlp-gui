use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;

use rusqlite::Connection;
use yt_dlp_gui::storage::schema::{initialize_schema, DOWNLOAD_SCHEMA_VERSION};
use yt_dlp_gui::storage::{
    DownloadProgress, DownloadStreamMediaType, DownloadTaskDraft, DownloadTaskStatus, DownloadTaskStream,
    DownloadTaskStreamDraft, Storage, StorageError,
};

fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "
pragma foreign_keys = on;
",
        )
        .unwrap();
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
        .query_row(
            "
select
    count(*)
from
    config
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let version: i64 = connection
        .query_row(
            "
select
    version
from
    storage_schema_versions
where
    domain = 'download_tasks'
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let table_count: i64 = connection
        .query_row(
            "
select
    count(*)
from
    sqlite_master
where
    type = 'table'
    and name in (
        'download_tasks',
        'download_task_streams'
    )
",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(config_count, 0);
    assert_eq!(version, DOWNLOAD_SCHEMA_VERSION);
    assert_eq!(table_count, 2);
    assert_eq!(
        connection
            .query_row(
                "
pragma foreign_keys
",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn repeated_schema_initialization_is_idempotent() {
    let connection = connection();
    initialize_schema(&connection).unwrap();
    let versions: i64 = connection
        .query_row(
            "
select
    count(*)
from
    storage_schema_versions
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(versions, 1);
}

#[test]
fn task_and_stream_rows_round_trip_and_cascade() {
    let connection = connection();
    let mut task_connection = connection;
    let draft = task(100);
    let transaction = task_connection.transaction().unwrap();
    transaction
        .execute(
            "
insert into download_tasks (
    source_url,
    video_id,
    title,
    target_path,
    status,
    created_at,
    updated_at
)
values (
    ?1,
    ?2,
    ?3,
    ?4,
    'pending',
    ?5,
    ?5
)
",
            rusqlite::params![
                draft.source_url,
                draft.video_id,
                draft.title,
                draft.target_path,
                draft.created_at,
            ],
        )
        .unwrap();
    let task_id = transaction.last_insert_rowid();
    transaction
        .execute(
            "
insert into download_task_streams (
    task_id,
    stream_key,
    media_type,
    status,
    created_at,
    updated_at
)
values (
    ?1,
    'video',
    'video',
    'pending',
    100,
    100
)
",
            [task_id],
        )
        .unwrap();
    transaction.commit().unwrap();

    assert_eq!(
        task_connection
            .query_row(
                "
select
    count(*)
from
    download_task_streams
where
    task_id = ?1
",
                [task_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    task_connection
        .execute(
            "
delete from
    download_tasks
where
    id = ?1
",
            [task_id],
        )
        .unwrap();
    assert_eq!(
        task_connection
            .query_row(
                "
select
    count(*)
from
    download_task_streams
",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn schema_rejects_invalid_progress_and_media_type() {
    let connection = connection();
    let error = connection
        .execute(
            "
insert into download_tasks (
    source_url,
    target_path,
    status,
    progress_percent,
    created_at,
    updated_at
)
values (
    'url',
    'path',
    'pending',
    101,
    1,
    1
)
",
            [],
        )
        .unwrap_err();
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
    assert!(!DownloadTaskStatus::Pending.is_terminal());
}

#[test]
fn duplicate_stream_key_is_rejected() {
    let connection = connection();
    connection
        .execute(
            "
insert into download_tasks (
    source_url,
    target_path,
    status,
    created_at,
    updated_at
)
values (
    'url',
    'path',
    'pending',
    1,
    1
)
",
            [],
        )
        .unwrap();
    let id = connection.last_insert_rowid();
    let sql = "
insert into download_task_streams (
    task_id,
    stream_key,
    media_type,
    status,
    created_at,
    updated_at
)
values (
    ?1,
    'same',
    'video',
    'pending',
    1,
    1
)
";
    connection.execute(sql, [id]).unwrap();
    assert!(connection.execute(sql, [id]).is_err());
}

static PUBLIC_STORAGE: OnceLock<&'static Storage> = OnceLock::new();
static NEXT_STREAM_SEQUENCE: AtomicI64 = AtomicI64::new(1_000);

fn public_storage() -> &'static Storage {
    PUBLIC_STORAGE.get_or_init(|| {
        let path = PathBuf::from("target").join(format!("storage-stream-state-{}.sqlite", std::process::id()));
        let _ = fs::remove_file(&path);
        Storage::initialize(path).unwrap();
        Storage::instance().unwrap()
    })
}

fn create_public_stream() -> (i64, i64) {
    let sequence = NEXT_STREAM_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let storage = public_storage();
    let task = storage
        .create_download_task(
            task(sequence),
            vec![DownloadTaskStreamDraft {
                stream_key: format!("stream-{sequence}"),
                format_id: Some("137".to_owned()),
                media_type: DownloadStreamMediaType::Video,
                extension: Some("mp4".to_owned()),
                width: Some(1_920),
                height: Some(1_080),
                video_codec: Some("avc1".to_owned()),
                audio_codec: None,
                created_at: sequence,
            }],
        )
        .unwrap();
    let stored = storage.get_download_task(task.id).unwrap().unwrap();
    (task.id, stored.streams[0].id)
}

fn stream_snapshot(task_id: i64, stream_id: i64) -> DownloadTaskStream {
    public_storage()
        .get_download_task(task_id)
        .unwrap()
        .unwrap()
        .streams
        .into_iter()
        .find(|stream| stream.id == stream_id)
        .unwrap()
}

fn advance_stream_to(stream_id: i64, status: DownloadTaskStatus, base_time: i64) {
    let storage = public_storage();
    match status {
        DownloadTaskStatus::Pending => {}
        DownloadTaskStatus::Preparing => storage
            .update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, base_time)
            .unwrap(),
        DownloadTaskStatus::Downloading => {
            storage
                .update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, base_time)
                .unwrap();
            storage
                .update_download_stream_status(stream_id, DownloadTaskStatus::Downloading, base_time + 1)
                .unwrap();
        }
        _ => panic!("测试辅助函数只接受非终态下载流状态"),
    }
}

fn valid_progress(updated_at: i64) -> DownloadProgress {
    DownloadProgress {
        progress_percent: Some(50),
        downloaded_bytes: 50,
        total_bytes: Some(100),
        total_bytes_estimate: None,
        speed_bytes_per_second: Some(10),
        elapsed_seconds: Some(5),
        eta_seconds: Some(5),
        updated_at,
    }
}

fn assert_invalid_transition(result: Result<(), StorageError>) {
    assert!(matches!(result, Err(StorageError::InvalidDownloadStatusTransition)));
}

fn assert_stream_not_found(result: Result<(), StorageError>, expected_id: i64) {
    assert!(matches!(
        result,
        Err(StorageError::DownloadStreamNotFound(id)) if id == expected_id
    ));
}

#[test]
fn public_task_delete_removes_records_without_deleting_local_file() {
    let storage = public_storage();
    let (task_id, _) = create_public_stream();
    let local_file = std::env::temp_dir().join(format!("yt-dlp-gui-task-delete-{}", task_id));
    fs::write(&local_file, b"fixture").unwrap();

    storage.delete_download_tasks(&[task_id]).unwrap();

    assert!(storage.get_download_task(task_id).unwrap().is_none());
    assert!(local_file.is_file());
    fs::remove_file(local_file).unwrap();
}
#[test]
fn public_stream_api_tracks_normal_lifecycle_timestamps() {
    let storage = public_storage();
    let (task_id, stream_id) = create_public_stream();

    storage
        .update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, 10)
        .unwrap();
    let preparing = stream_snapshot(task_id, stream_id);
    assert_eq!(preparing.status, DownloadTaskStatus::Preparing);
    assert_eq!(preparing.started_at, None);
    assert_eq!(preparing.finished_at, None);
    assert_eq!(preparing.updated_at, 10);

    storage
        .update_download_stream_status(stream_id, DownloadTaskStatus::Downloading, 20)
        .unwrap();
    let downloading = stream_snapshot(task_id, stream_id);
    assert_eq!(downloading.status, DownloadTaskStatus::Downloading);
    assert_eq!(downloading.started_at, Some(20));
    assert_eq!(downloading.finished_at, None);
    assert_eq!(downloading.updated_at, 20);

    storage.complete_download_stream(stream_id, 30).unwrap();
    let completed = stream_snapshot(task_id, stream_id);
    assert_eq!(completed.status, DownloadTaskStatus::Completed);
    assert_eq!(completed.started_at, Some(20));
    assert_eq!(completed.finished_at, Some(30));
    assert_eq!(completed.updated_at, 30);
    assert_eq!(completed.progress_percent, None);
    assert_eq!(completed.downloaded_bytes, 0);
}

#[test]
fn public_stream_api_allows_cancel_and_failure_from_each_active_stage() {
    let storage = public_storage();
    let stages = [
        DownloadTaskStatus::Pending,
        DownloadTaskStatus::Preparing,
        DownloadTaskStatus::Downloading,
    ];

    for (index, stage) in stages.into_iter().enumerate() {
        let (task_id, stream_id) = create_public_stream();
        let base_time = 100 + index as i64 * 10;
        advance_stream_to(stream_id, stage, base_time);
        storage.cancel_download_stream(stream_id, base_time + 5).unwrap();
        let stream = stream_snapshot(task_id, stream_id);
        assert_eq!(stream.status, DownloadTaskStatus::Cancelled);
        assert_eq!(stream.finished_at, Some(base_time + 5));
        assert_eq!(stream.updated_at, base_time + 5);
    }

    for (index, stage) in stages.into_iter().enumerate() {
        let (task_id, stream_id) = create_public_stream();
        let base_time = 200 + index as i64 * 10;
        advance_stream_to(stream_id, stage, base_time);
        storage.fail_download_stream(stream_id, base_time + 5).unwrap();
        let stream = stream_snapshot(task_id, stream_id);
        assert_eq!(stream.status, DownloadTaskStatus::Failed);
        assert_eq!(stream.finished_at, Some(base_time + 5));
        assert_eq!(stream.updated_at, base_time + 5);
    }
}

#[test]
fn public_stream_api_rejects_invalid_transitions_without_mutation() {
    let storage = public_storage();

    let (completed_task, completed_stream) = create_public_stream();
    advance_stream_to(completed_stream, DownloadTaskStatus::Downloading, 300);
    storage.complete_download_stream(completed_stream, 305).unwrap();
    let completed = stream_snapshot(completed_task, completed_stream);
    assert_invalid_transition(storage.update_download_stream_status(
        completed_stream,
        DownloadTaskStatus::Downloading,
        306,
    ));
    assert_invalid_transition(storage.fail_download_stream(completed_stream, 307));
    assert_eq!(stream_snapshot(completed_task, completed_stream), completed);

    let (cancelled_task, cancelled_stream) = create_public_stream();
    storage.cancel_download_stream(cancelled_stream, 310).unwrap();
    let cancelled = stream_snapshot(cancelled_task, cancelled_stream);
    assert_invalid_transition(storage.complete_download_stream(cancelled_stream, 311));
    assert_eq!(stream_snapshot(cancelled_task, cancelled_stream), cancelled);

    let (failed_task, failed_stream) = create_public_stream();
    storage.fail_download_stream(failed_stream, 320).unwrap();
    let failed = stream_snapshot(failed_task, failed_stream);
    assert_invalid_transition(storage.update_download_stream_status(
        failed_stream,
        DownloadTaskStatus::Downloading,
        321,
    ));
    assert_eq!(stream_snapshot(failed_task, failed_stream), failed);

    let (downloading_task, downloading_stream) = create_public_stream();
    advance_stream_to(downloading_stream, DownloadTaskStatus::Downloading, 330);
    let downloading = stream_snapshot(downloading_task, downloading_stream);
    assert_invalid_transition(storage.update_download_stream_status(
        downloading_stream,
        DownloadTaskStatus::Downloading,
        332,
    ));
    assert_invalid_transition(storage.update_download_stream_status(
        downloading_stream,
        DownloadTaskStatus::Preparing,
        333,
    ));
    assert_invalid_transition(storage.update_download_stream_status(
        downloading_stream,
        DownloadTaskStatus::Merging,
        334,
    ));
    assert_eq!(stream_snapshot(downloading_task, downloading_stream), downloading);
}

#[test]
fn public_stream_api_blocks_progress_after_each_terminal_status() {
    let storage = public_storage();
    let terminal_statuses = [
        DownloadTaskStatus::Completed,
        DownloadTaskStatus::Cancelled,
        DownloadTaskStatus::Failed,
    ];

    for (index, terminal_status) in terminal_statuses.into_iter().enumerate() {
        let (task_id, stream_id) = create_public_stream();
        let base_time = 400 + index as i64 * 10;
        match terminal_status {
            DownloadTaskStatus::Completed => {
                advance_stream_to(stream_id, DownloadTaskStatus::Downloading, base_time);
                storage.complete_download_stream(stream_id, base_time + 5).unwrap();
            }
            DownloadTaskStatus::Cancelled => {
                storage.cancel_download_stream(stream_id, base_time + 5).unwrap();
            }
            DownloadTaskStatus::Failed => {
                storage.fail_download_stream(stream_id, base_time + 5).unwrap();
            }
            _ => unreachable!(),
        }
        let before = stream_snapshot(task_id, stream_id);
        assert!(matches!(
            storage.update_download_stream_progress(stream_id, valid_progress(base_time + 6)),
            Err(StorageError::InvalidDownloadProgress)
        ));
        assert_eq!(stream_snapshot(task_id, stream_id), before);
    }
}

#[test]
fn public_stream_api_reports_unknown_stream_ids() {
    let storage = public_storage();
    let unknown_id = i64::MAX;

    assert_stream_not_found(
        storage.update_download_stream_status(unknown_id, DownloadTaskStatus::Preparing, 500),
        unknown_id,
    );
    assert_stream_not_found(storage.complete_download_stream(unknown_id, 501), unknown_id);
    assert_stream_not_found(storage.fail_download_stream(unknown_id, 502), unknown_id);
    assert_stream_not_found(storage.cancel_download_stream(unknown_id, 503), unknown_id);
    assert_stream_not_found(
        storage.update_download_stream_progress(unknown_id, valid_progress(504)),
        unknown_id,
    );
}

#[test]
fn public_stream_api_rejects_negative_timestamps_without_mutation() {
    let storage = public_storage();
    let (task_id, stream_id) = create_public_stream();
    let before = stream_snapshot(task_id, stream_id);

    assert!(matches!(
        storage.update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, -1),
        Err(StorageError::InvalidDownloadInput)
    ));
    assert!(matches!(
        storage.cancel_download_stream(stream_id, -1),
        Err(StorageError::InvalidDownloadInput)
    ));
    assert_eq!(stream_snapshot(task_id, stream_id), before);
}
