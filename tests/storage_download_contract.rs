use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::OnceLock;

use rusqlite::Connection;
use yt_dlp_gui::storage::schema::{initialize_schema, DOWNLOAD_SCHEMA_VERSION};
use yt_dlp_gui::storage::{
    DownloadExecutionOptions, DownloadExecutionSnapshot, DownloadProgress, DownloadStreamMediaType, DownloadTask,
    DownloadTaskDraft, DownloadTaskStatus, DownloadTaskStream, DownloadTaskStreamDraft, Storage, StorageError,
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

fn execution_snapshot() -> DownloadExecutionSnapshot {
    DownloadExecutionSnapshot {
        source_url: "https://example.invalid/video".to_owned(),
        video_format_id: "137".to_owned(),
        audio_format_id: "140".to_owned(),
        output_template: "%(title)s.%(ext)s".to_owned(),
        target_directory: "C:/Downloads".to_owned(),
        temporary_directory: "C:/Downloads/.yt-dlp-gui-temp".to_owned(),
        merge_output_format: "mp4".to_owned(),
        options: DownloadExecutionOptions {
            rate_limit: Some("2M".to_owned()),
            retries: Some(3),
            fragment_retries: Some(4),
            file_access_retries: Some(5),
            concurrent_fragments: Some(2),
        },
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
        'download_task_streams',
        'download_task_execution_snapshots'
    )
",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(config_count, 0);
    assert_eq!(version, DOWNLOAD_SCHEMA_VERSION);
    assert_eq!(table_count, 3);
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
        .create_download_task_with_execution_snapshot(
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
            execution_snapshot(),
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

fn task_snapshot(task_id: i64) -> DownloadTask {
    public_storage().get_download_task(task_id).unwrap().unwrap().task
}

fn stream_draft(stream_key: String, media_type: DownloadStreamMediaType, created_at: i64) -> DownloadTaskStreamDraft {
    DownloadTaskStreamDraft {
        stream_key,
        format_id: Some("140".to_owned()),
        media_type,
        extension: Some("m4a".to_owned()),
        width: None,
        height: None,
        video_codec: None,
        audio_codec: Some("mp4a".to_owned()),
        created_at,
    }
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

#[test]
fn paused_tasks_cannot_resume_through_generic_apis_or_accept_new_streams() {
    let storage = public_storage();
    let (task_id, stream_id) = create_public_stream();
    assert_invalid_transition(storage.pause_download_task(task_id, 10));
    storage
        .update_download_status(task_id, DownloadTaskStatus::Preparing, 11)
        .unwrap();
    assert_invalid_transition(storage.update_download_status(task_id, DownloadTaskStatus::Paused, 12));
    storage.pause_download_task(task_id, 12).unwrap();
    let before = storage.get_download_task(task_id).unwrap().unwrap();
    assert_invalid_transition(storage.pause_download_task(task_id, 13));
    assert_invalid_transition(storage.update_download_status(task_id, DownloadTaskStatus::Preparing, 13));
    assert_invalid_transition(storage.update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, 13));
    assert_invalid_transition(storage.update_download_stream_status(stream_id, DownloadTaskStatus::Downloading, 13));
    assert!(matches!(
        storage.create_download_stream(
            task_id,
            stream_draft("new".to_owned(), DownloadStreamMediaType::Audio, 13),
        ),
        Err(StorageError::InvalidDownloadStatusTransition)
    ));
    assert!(matches!(
        storage.pause_download_task(task_id, -1),
        Err(StorageError::InvalidDownloadInput)
    ));
    assert!(matches!(
        storage.prepare_resumed_download(task_id, -1),
        Err(StorageError::InvalidDownloadInput)
    ));
    assert_eq!(storage.get_download_task(task_id).unwrap().unwrap(), before);
    storage.prepare_resumed_download(task_id, 14).unwrap();
    assert_invalid_transition(storage.prepare_resumed_download(task_id, 15));
}

#[test]
fn resume_errors_distinguish_missing_task_snapshot_and_terminal_status() {
    let storage = public_storage();
    assert!(matches!(
        storage.prepare_resumed_download(i64::MAX, 10),
        Err(StorageError::DownloadNotFound(_))
    ));
    assert!(matches!(
        storage.load_download_execution_snapshot(i64::MAX),
        Err(StorageError::DownloadNotFound(_))
    ));
    let legacy = storage.create_download_task(task(10), vec![]).unwrap();
    assert_eq!(storage.load_download_execution_snapshot(legacy.id).unwrap(), None);
    storage
        .update_download_status(legacy.id, DownloadTaskStatus::Preparing, 11)
        .unwrap();
    storage.pause_download_task(legacy.id, 12).unwrap();
    assert!(matches!(
        storage.prepare_resumed_download(legacy.id, 13),
        Err(StorageError::DownloadExecutionSnapshotMissing(_))
    ));
    assert_invalid_transition(storage.update_download_status(legacy.id, DownloadTaskStatus::Preparing, 13));
    assert_eq!(task_snapshot(legacy.id).status, DownloadTaskStatus::Paused);

    for terminal in [
        DownloadTaskStatus::Completed,
        DownloadTaskStatus::Cancelled,
        DownloadTaskStatus::Failed,
    ] {
        let (task_id, stream_id) = create_public_stream();
        storage
            .update_download_status(task_id, DownloadTaskStatus::Preparing, 10)
            .unwrap();
        if terminal == DownloadTaskStatus::Completed {
            storage
                .update_download_status(task_id, DownloadTaskStatus::Downloading, 11)
                .unwrap();
            advance_stream_to(stream_id, DownloadTaskStatus::Downloading, 11);
            storage.complete_download_stream(stream_id, 13).unwrap();
            storage
                .complete_download_task(task_id, "output.mp4".to_owned(), 13)
                .unwrap();
        } else {
            storage.pause_download_task(task_id, 11).unwrap();
            storage.update_download_stream_status(stream_id, terminal, 13).unwrap();
            if terminal == DownloadTaskStatus::Failed {
                storage
                    .fail_download_task(task_id, None, "failure".to_owned(), 13)
                    .unwrap();
            } else {
                storage.cancel_download_task(task_id, 13).unwrap();
            }
        }
        let before = storage.get_download_task(task_id).unwrap().unwrap();
        assert_invalid_transition(storage.prepare_resumed_download(task_id, 14));
        assert_invalid_transition(storage.pause_download_task(task_id, 14));
        assert_invalid_transition(storage.update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, 14));
        assert!(matches!(
            storage.update_download_progress(task_id, valid_progress(14)),
            Err(StorageError::InvalidDownloadProgress)
        ));
        assert_eq!(storage.get_download_task(task_id).unwrap().unwrap(), before);
    }
}

#[test]
fn pause_resume_and_recovery_roll_back_task_when_stream_update_fails() {
    if run_in_child("pause_resume_and_recovery_roll_back_task_when_stream_update_fails") {
        return;
    }
    let storage = public_storage();
    let (task_id, _) = create_public_stream();
    storage
        .update_download_status(task_id, DownloadTaskStatus::Preparing, 10)
        .unwrap();
    let connection = Connection::open(storage.database_path()).unwrap();
    let reject_updates = "
create trigger reject_stream_update
before update on download_task_streams
begin
    select raise(abort, 'fixture failure');
end;
";
    let allow_updates = "
drop trigger reject_stream_update;
";
    let before = storage.get_download_task(task_id).unwrap().unwrap();
    connection.execute_batch(reject_updates).unwrap();
    assert!(storage.pause_download_task(task_id, 11).is_err());
    assert_eq!(storage.get_download_task(task_id).unwrap().unwrap(), before);
    assert!(storage.recover_interrupted_downloads(12).is_err());
    assert_eq!(storage.get_download_task(task_id).unwrap().unwrap(), before);
    connection.execute_batch(allow_updates).unwrap();
    storage.pause_download_task(task_id, 13).unwrap();
    let paused = storage.get_download_task(task_id).unwrap().unwrap();
    connection.execute_batch(reject_updates).unwrap();
    assert!(storage.prepare_resumed_download(task_id, 14).is_err());
    assert_eq!(storage.get_download_task(task_id).unwrap().unwrap(), paused);
    connection.execute_batch(allow_updates).unwrap();
    storage.prepare_resumed_download(task_id, 15).unwrap();
    assert_eq!(task_snapshot(task_id).status, DownloadTaskStatus::Preparing);
}

#[test]
fn snapshot_creation_is_atomic_validated_and_cascades_on_delete() {
    if run_in_child("snapshot_creation_is_atomic_validated_and_cascades_on_delete") {
        return;
    }
    let storage = public_storage();
    let connection = Connection::open(storage.database_path()).unwrap();
    let stream = stream_draft("audio".to_owned(), DownloadStreamMediaType::Audio, 10);
    assert!(storage
        .create_download_task_with_execution_snapshot(
            task(10),
            vec![stream.clone(), stream.clone()],
            execution_snapshot(),
        )
        .is_err());
    let counts: (i64, i64, i64) = connection
        .query_row(
            "
select
    (
        select
            count(*)
        from
            download_tasks
    ),
    (
        select
            count(*)
        from
            download_task_streams
    ),
    (
        select
            count(*)
        from
            download_task_execution_snapshots
    )
",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
    let mut wrong = execution_snapshot();
    wrong.source_url = "different".to_owned();
    assert!(matches!(
        storage.create_download_task_with_execution_snapshot(task(10), vec![], wrong),
        Err(StorageError::InvalidDownloadInput)
    ));
    let mut wrong_stream = stream.clone();
    wrong_stream.format_id = Some("999".to_owned());
    assert!(matches!(
        storage.create_download_task_with_execution_snapshot(task(10), vec![wrong_stream], execution_snapshot()),
        Err(StorageError::InvalidDownloadInput)
    ));
    let created = storage
        .create_download_task_with_execution_snapshot(task(10), vec![stream], execution_snapshot())
        .unwrap();
    storage.delete_download_tasks(&[created.id]).unwrap();
    let snapshots: i64 = connection
        .query_row(
            "
select
    count(*)
from
    download_task_execution_snapshots
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(snapshots, 0);
}

#[test]
fn execution_snapshot_round_trips_and_cannot_be_updated() {
    let storage = public_storage();
    let (task_id, _) = create_public_stream();

    assert_eq!(
        storage.load_download_execution_snapshot(task_id).unwrap(),
        Some(execution_snapshot())
    );

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
    let legacy_task_id = connection.last_insert_rowid();
    connection
        .execute(
            "
insert into download_task_execution_snapshots (
    task_id,
    source_url,
    video_format_id,
    audio_format_id,
    output_template,
    target_directory,
    temporary_directory,
    merge_output_format
)
values (
    ?1,
    'url',
    '137',
    '140',
    '%(title)s.%(ext)s',
    'path',
    'temp',
    'mp4'
)
",
            [legacy_task_id],
        )
        .unwrap();
    assert!(connection
        .execute(
            "
update
    download_task_execution_snapshots
set
    output_template = 'changed'
where
    task_id = ?1
",
            [legacy_task_id],
        )
        .is_err());
}

#[test]
fn pause_and_resume_preserve_progress_and_completed_streams() {
    let storage = public_storage();
    let (task_id, video_stream_id) = create_public_stream();
    let audio_stream = storage
        .create_download_stream(
            task_id,
            stream_draft(format!("audio-{task_id}"), DownloadStreamMediaType::Audio, 10),
        )
        .unwrap();

    storage
        .update_download_status(task_id, DownloadTaskStatus::Preparing, 10)
        .unwrap();
    storage
        .update_download_status(task_id, DownloadTaskStatus::Downloading, 11)
        .unwrap();
    advance_stream_to(video_stream_id, DownloadTaskStatus::Downloading, 11);
    storage
        .update_download_stream_progress(video_stream_id, valid_progress(12))
        .unwrap();
    storage.complete_download_stream(video_stream_id, 13).unwrap();
    advance_stream_to(audio_stream.id, DownloadTaskStatus::Downloading, 14);
    storage
        .update_download_stream_progress(audio_stream.id, valid_progress(15))
        .unwrap();
    storage.update_download_progress(task_id, valid_progress(15)).unwrap();

    let completed_video = stream_snapshot(task_id, video_stream_id);
    let active_audio = stream_snapshot(task_id, audio_stream.id);
    storage.pause_download_task(task_id, 20).unwrap();

    let paused_task = task_snapshot(task_id);
    let paused_audio = stream_snapshot(task_id, audio_stream.id);
    assert_eq!(paused_task.status, DownloadTaskStatus::Paused);
    assert_eq!(paused_task.id, task_id);
    assert_eq!(paused_task.downloaded_bytes, 50);
    assert_eq!(paused_task.total_bytes, Some(100));
    assert_eq!(paused_task.started_at, Some(11));
    assert_eq!(paused_task.finished_at, None);
    assert_eq!(paused_task.speed_bytes_per_second, None);
    assert_eq!(paused_task.elapsed_seconds, None);
    assert_eq!(paused_task.eta_seconds, None);
    assert_eq!(paused_audio.status, DownloadTaskStatus::Paused);
    assert_eq!(paused_audio.downloaded_bytes, active_audio.downloaded_bytes);
    assert_eq!(paused_audio.total_bytes, active_audio.total_bytes);
    assert_eq!(paused_audio.started_at, active_audio.started_at);
    assert_eq!(paused_audio.finished_at, None);
    assert_eq!(paused_audio.speed_bytes_per_second, None);
    assert_eq!(stream_snapshot(task_id, video_stream_id), completed_video);

    assert!(matches!(
        storage.update_download_progress(task_id, valid_progress(21)),
        Err(StorageError::InvalidDownloadProgress)
    ));
    assert!(matches!(
        storage.update_download_stream_progress(audio_stream.id, valid_progress(21)),
        Err(StorageError::InvalidDownloadProgress)
    ));
    assert_eq!(task_snapshot(task_id), paused_task);
    assert_eq!(stream_snapshot(task_id, audio_stream.id), paused_audio);

    storage.prepare_resumed_download(task_id, 22).unwrap();
    let resumed_task = task_snapshot(task_id);
    let resumed_audio = stream_snapshot(task_id, audio_stream.id);
    assert_eq!(resumed_task.id, task_id);
    assert_eq!(resumed_task.status, DownloadTaskStatus::Preparing);
    assert_eq!(resumed_task.downloaded_bytes, paused_task.downloaded_bytes);
    assert_eq!(resumed_task.started_at, paused_task.started_at);
    assert_eq!(resumed_task.finished_at, None);
    assert_eq!(resumed_audio.status, DownloadTaskStatus::Preparing);
    assert_eq!(resumed_audio.downloaded_bytes, paused_audio.downloaded_bytes);
    assert_eq!(resumed_audio.started_at, paused_audio.started_at);
    assert_eq!(stream_snapshot(task_id, video_stream_id), completed_video);
}

#[test]
fn pause_rejects_merging_and_cancelled_tasks_cannot_resume() {
    let storage = public_storage();
    let (merging_task_id, _) = create_public_stream();
    storage
        .update_download_status(merging_task_id, DownloadTaskStatus::Preparing, 10)
        .unwrap();
    storage
        .update_download_status(merging_task_id, DownloadTaskStatus::Downloading, 11)
        .unwrap();
    storage
        .update_download_status(merging_task_id, DownloadTaskStatus::Merging, 12)
        .unwrap();
    let merging = task_snapshot(merging_task_id);
    assert_invalid_transition(storage.pause_download_task(merging_task_id, 13));
    assert_eq!(task_snapshot(merging_task_id), merging);

    let (paused_task_id, paused_stream_id) = create_public_stream();
    storage
        .update_download_status(paused_task_id, DownloadTaskStatus::Preparing, 20)
        .unwrap();
    storage.pause_download_task(paused_task_id, 21).unwrap();
    storage.cancel_download_stream(paused_stream_id, 22).unwrap();
    storage.cancel_download_task(paused_task_id, 22).unwrap();
    assert_eq!(task_snapshot(paused_task_id).status, DownloadTaskStatus::Cancelled);
    assert_eq!(
        stream_snapshot(paused_task_id, paused_stream_id).status,
        DownloadTaskStatus::Cancelled
    );
    assert_invalid_transition(storage.prepare_resumed_download(paused_task_id, 23));
}

#[test]
fn interrupted_active_tasks_recover_to_paused_without_rewriting_completed_streams() {
    // 全局恢复只能在独立进程中验证，不得修改其他并行用例的活动任务。
    if run_in_child("interrupted_active_tasks_recover_to_paused_without_rewriting_completed_streams") {
        return;
    }
    let storage = public_storage();
    let (preparing_task_id, preparing_stream_id) = create_public_stream();
    storage
        .update_download_status(preparing_task_id, DownloadTaskStatus::Preparing, 10)
        .unwrap();

    let (downloading_task_id, downloading_stream_id) = create_public_stream();
    storage
        .update_download_status(downloading_task_id, DownloadTaskStatus::Preparing, 11)
        .unwrap();
    storage
        .update_download_status(downloading_task_id, DownloadTaskStatus::Downloading, 12)
        .unwrap();
    advance_stream_to(downloading_stream_id, DownloadTaskStatus::Downloading, 12);
    storage
        .update_download_stream_progress(downloading_stream_id, valid_progress(13))
        .unwrap();
    storage
        .update_download_progress(downloading_task_id, valid_progress(13))
        .unwrap();

    let (merging_task_id, merging_stream_id) = create_public_stream();
    storage
        .update_download_status(merging_task_id, DownloadTaskStatus::Preparing, 14)
        .unwrap();
    storage
        .update_download_status(merging_task_id, DownloadTaskStatus::Downloading, 15)
        .unwrap();
    advance_stream_to(merging_stream_id, DownloadTaskStatus::Downloading, 15);
    storage.complete_download_stream(merging_stream_id, 16).unwrap();
    storage
        .update_download_status(merging_task_id, DownloadTaskStatus::Merging, 17)
        .unwrap();
    let completed_stream = stream_snapshot(merging_task_id, merging_stream_id);

    assert_eq!(storage.recover_interrupted_downloads(30).unwrap(), 3);
    assert_eq!(task_snapshot(preparing_task_id).status, DownloadTaskStatus::Paused);
    assert_eq!(
        stream_snapshot(preparing_task_id, preparing_stream_id).status,
        DownloadTaskStatus::Paused
    );
    let recovered_downloading = task_snapshot(downloading_task_id);
    assert_eq!(recovered_downloading.status, DownloadTaskStatus::Paused);
    assert_eq!(recovered_downloading.downloaded_bytes, 50);
    assert_eq!(recovered_downloading.started_at, Some(12));
    assert_eq!(recovered_downloading.speed_bytes_per_second, None);
    assert_eq!(
        stream_snapshot(downloading_task_id, downloading_stream_id).status,
        DownloadTaskStatus::Paused
    );
    assert_eq!(task_snapshot(merging_task_id).status, DownloadTaskStatus::Paused);
    assert_eq!(stream_snapshot(merging_task_id, merging_stream_id), completed_stream);
    assert_eq!(storage.recover_interrupted_downloads(31).unwrap(), 0);
}

fn run_in_child(name: &str) -> bool {
    if std::env::var("STORAGE_CONTRACT_CHILD").as_deref() == Ok(name) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("STORAGE_CONTRACT_CHILD", name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "子进程验证失败：{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

fn initialize_legacy_downloads(connection: &Connection) {
    connection
        .execute_batch(
            "
pragma foreign_keys = on;

create table storage_schema_versions (
    domain text primary key,
    version integer not null
);

create table download_tasks (
    id integer primary key autoincrement,
    source_url text not null,
    video_id text,
    title text,
    thumbnail_url text,
    duration_seconds integer,
    target_path text not null,
    output_path text,
    selected_format text,
    status text not null check (
        status in (
            'pending',
            'preparing',
            'downloading',
            'merging',
            'completed',
            'cancelled',
            'failed'
        )
    ),
    progress_percent integer,
    downloaded_bytes integer not null default 0,
    total_bytes integer,
    total_bytes_estimate integer,
    speed_bytes_per_second integer,
    elapsed_seconds integer,
    eta_seconds integer,
    created_at integer not null,
    started_at integer,
    finished_at integer,
    updated_at integer not null,
    yt_dlp_version text,
    error_code text,
    error_message text
);

create table download_task_streams (
    id integer primary key autoincrement,
    task_id integer not null,
    stream_key text not null,
    format_id text,
    media_type text not null,
    extension text,
    width integer,
    height integer,
    video_codec text,
    audio_codec text,
    status text not null check (
        status in (
            'pending',
            'preparing',
            'downloading',
            'merging',
            'completed',
            'cancelled',
            'failed'
        )
    ),
    progress_percent integer,
    downloaded_bytes integer not null default 0,
    total_bytes integer,
    total_bytes_estimate integer,
    speed_bytes_per_second integer,
    elapsed_seconds integer,
    eta_seconds integer,
    created_at integer not null,
    started_at integer,
    finished_at integer,
    updated_at integer not null,
    foreign key (task_id) references download_tasks(id) on delete cascade,
    unique (task_id, stream_key)
);

insert into storage_schema_versions (
    domain,
    version
)
values (
    'download_tasks',
    1
);

insert into download_tasks (
    id,
    source_url,
    target_path,
    selected_format,
    status,
    downloaded_bytes,
    created_at,
    started_at,
    updated_at
)
values (
    7,
    'https://example.invalid/legacy',
    'C:/Downloads',
    '137+140',
    'downloading',
    50,
    10,
    11,
    12
);

insert into download_task_streams (
    id,
    task_id,
    stream_key,
    format_id,
    media_type,
    status,
    downloaded_bytes,
    created_at,
    started_at,
    updated_at
)
values (
    9,
    7,
    '137',
    '137',
    'video',
    'downloading',
    50,
    10,
    11,
    12
);
",
        )
        .unwrap();
}

#[test]
fn migration_preserves_indexes_sequences_and_rejects_stream_merging() {
    let connection = Connection::open_in_memory().unwrap();
    initialize_legacy_downloads(&connection);
    connection
        .execute_batch(
            "
create index idx_download_tasks_status_updated
    on download_tasks(status, updated_at desc, id desc);
create index idx_download_tasks_created
    on download_tasks(created_at desc, id desc);
create index idx_download_task_streams_task
    on download_task_streams(task_id);

update
    sqlite_sequence
set
    seq = 500
where
    name in ('download_tasks', 'download_task_streams');

update
    download_task_streams
set
    status = 'merging',
    speed_bytes_per_second = 10,
    elapsed_seconds = 5,
    eta_seconds = 2
where
    id = 9;
",
        )
        .unwrap();
    initialize_schema(&connection).unwrap();
    let indexes: i64 = connection
        .query_row(
            "
select
    count(*)
from
    sqlite_master
where
    type = 'index'
    and name in (
        'idx_download_tasks_status_updated',
        'idx_download_tasks_created',
        'idx_download_task_streams_task'
    )
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 3);
    let stream: (String, i64, Option<i64>, Option<i64>) = connection
        .query_row(
            "
select
    status,
    downloaded_bytes,
    started_at,
    speed_bytes_per_second
from
    download_task_streams
where
    id = 9
",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(stream, ("paused".to_owned(), 50, Some(11), None));
    assert!(connection
        .execute(
            "
update
    download_task_streams
set
    status = 'merging'
where
    id = 9
",
            [],
        )
        .is_err());
    connection
        .execute_batch(
            "
insert into download_tasks (
    source_url,
    target_path,
    status,
    created_at,
    updated_at
)
values (
    'new',
    'path',
    'pending',
    20,
    20
);

insert into download_task_streams (
    task_id,
    stream_key,
    media_type,
    status,
    created_at,
    updated_at
)
values (
    501,
    'new',
    'audio',
    'pending',
    20,
    20
);
",
        )
        .unwrap();
    assert_eq!(connection.last_insert_rowid(), 501);
    initialize_schema(&connection).unwrap();
}

#[test]
fn failed_migration_rolls_back_before_commit_and_restores_foreign_keys() {
    let connection = Connection::open_in_memory().unwrap();
    initialize_legacy_downloads(&connection);
    connection
        .execute_batch(
            "
pragma foreign_keys = off;
update
    download_task_streams
set
    task_id = 999
where
    id = 9;
pragma foreign_keys = on;
",
        )
        .unwrap();
    assert!(initialize_schema(&connection).is_err());
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
    assert_eq!(version, 1);
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
    // 修复夹具的孤立外键后，同一连接应可以重新迁移。
    connection
        .execute(
            "
update
    download_task_streams
set
    task_id = 7
where
    id = 9
",
            [],
        )
        .unwrap();
    initialize_schema(&connection).unwrap();
}

#[test]
fn startup_recovers_legacy_tasks_and_repeated_initialization_does_not_touch_running_tasks() {
    if run_in_child("startup_recovers_legacy_tasks_and_repeated_initialization_does_not_touch_running_tasks") {
        return;
    }
    let path = std::env::temp_dir().join(format!("storage-startup-{}.sqlite", std::process::id()));
    let _ = fs::remove_file(&path);
    let connection = Connection::open(&path).unwrap();
    initialize_legacy_downloads(&connection);
    drop(connection);
    Storage::initialize(path.clone()).unwrap();
    let storage = Storage::instance().unwrap();
    let recovered = storage.get_download_task(7).unwrap().unwrap();
    assert_eq!(recovered.task.status, DownloadTaskStatus::Paused);
    assert_eq!(recovered.task.finished_at, None);
    assert_eq!(recovered.task.started_at, Some(11));
    assert_eq!(recovered.task.downloaded_bytes, 50);
    assert_eq!(recovered.streams[0].status, DownloadTaskStatus::Paused);
    assert_eq!(storage.load_download_execution_snapshot(7).unwrap(), None);
    assert!(matches!(
        storage.prepare_resumed_download(7, 30),
        Err(StorageError::DownloadExecutionSnapshotMissing(7))
    ));
    let running = storage
        .create_download_task_with_execution_snapshot(task(30), vec![], execution_snapshot())
        .unwrap();
    storage
        .update_download_status(running.id, DownloadTaskStatus::Preparing, 31)
        .unwrap();
    assert!(matches!(
        Storage::initialize(path),
        Err(StorageError::AlreadyInitialized)
    ));
    assert_eq!(
        storage.get_download_task(running.id).unwrap().unwrap().task.status,
        DownloadTaskStatus::Preparing
    );
}

#[test]
fn migrates_version_one_download_records_without_losing_rows() {
    let connection = Connection::open_in_memory().unwrap();
    initialize_legacy_downloads(&connection);
    initialize_schema(&connection).unwrap();

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
    let migrated_status: String = connection
        .query_row(
            "
select
    status
from
    download_tasks
where
    id = 7
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let migrated_stream_task_id: i64 = connection
        .query_row(
            "
select
    task_id
from
    download_task_streams
where
    id = 9
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let execution_snapshot_count: i64 = connection
        .query_row(
            "
select
    count(*)
from
    download_task_execution_snapshots
where
    task_id = 7
",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(version, DOWNLOAD_SCHEMA_VERSION);
    assert_eq!(migrated_status, "downloading");
    assert_eq!(migrated_stream_task_id, 7);
    assert_eq!(execution_snapshot_count, 0);
    let persisted: (i64, Option<i64>) = connection
        .query_row(
            "
select
    downloaded_bytes,
    started_at
from
    download_tasks
where
    id = 7
",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(persisted, (50, Some(11)));
    connection
        .execute(
            "
update
    download_tasks
set
    status = 'paused'
where
    id = 7
",
            [],
        )
        .unwrap();
}
