use rusqlite::{params, Connection, Transaction};

use super::super::download::{
    DownloadExecutionSnapshot, DownloadTask, DownloadTaskDraft, DownloadTaskStream, DownloadTaskStreamDraft,
};
use super::super::error::StorageError;
use super::read::{get_download_stream, get_download_task};
use super::support::map_download_write_error;

/// 在一个事务中写入任务和全部初始流，保证不会留下半成品任务。
pub(crate) fn create_download_task(
    connection: &mut Connection,
    draft: &DownloadTaskDraft,
    streams: &[DownloadTaskStreamDraft],
    execution_snapshot: Option<&DownloadExecutionSnapshot>,
) -> Result<DownloadTask, StorageError> {
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let task_id = insert_download_task(&transaction, draft).map_err(StorageError::Write)?;
    if let Some(execution_snapshot) = execution_snapshot {
        insert_download_execution_snapshot(&transaction, task_id, execution_snapshot).map_err(StorageError::Write)?;
    }
    for stream in streams {
        insert_download_stream(&transaction, task_id, stream).map_err(map_download_write_error)?;
    }
    transaction.commit().map_err(StorageError::Write)?;
    get_download_task(connection, task_id)?
        .map(|task| task.task)
        .ok_or(StorageError::DownloadNotFound(task_id))
}

fn insert_download_task(transaction: &Transaction<'_>, draft: &DownloadTaskDraft) -> rusqlite::Result<i64> {
    transaction.execute(
        "
insert into download_tasks (
    source_url,
    video_id,
    title,
    thumbnail_url,
    duration_seconds,
    target_path,
    output_path,
    selected_format,
    status,
    created_at,
    updated_at,
    yt_dlp_version
)
values (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    'pending',
    ?9,
    ?9,
    ?10
)
",
        params![
            draft.source_url,
            draft.video_id,
            draft.title,
            draft.thumbnail_url,
            draft.duration_seconds,
            draft.target_path,
            draft.output_path,
            draft.selected_format,
            draft.created_at,
            draft.yt_dlp_version,
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

fn insert_download_execution_snapshot(
    transaction: &Transaction<'_>,
    task_id: i64,
    snapshot: &DownloadExecutionSnapshot,
) -> rusqlite::Result<()> {
    transaction.execute(
        "
insert into download_task_execution_snapshots (
    task_id,
    source_url,
    video_format_id,
    audio_format_id,
    output_template,
    target_directory,
    temporary_directory,
    merge_output_format,
    rate_limit,
    retries,
    fragment_retries,
    file_access_retries,
    concurrent_fragments
)
values (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    ?10,
    ?11,
    ?12,
    ?13
)
",
        params![
            task_id,
            snapshot.source_url,
            snapshot.video_format_id,
            snapshot.audio_format_id,
            snapshot.output_template,
            snapshot.target_directory,
            snapshot.temporary_directory,
            snapshot.merge_output_format,
            snapshot.options.rate_limit,
            snapshot.options.retries,
            snapshot.options.fragment_retries,
            snapshot.options.file_access_retries,
            snapshot.options.concurrent_fragments,
        ],
    )?;
    Ok(())
}

fn insert_download_stream(
    transaction: &Transaction<'_>,
    task_id: i64,
    draft: &DownloadTaskStreamDraft,
) -> rusqlite::Result<i64> {
    transaction.execute(
        "
insert into download_task_streams (
    task_id,
    stream_key,
    format_id,
    media_type,
    extension,
    width,
    height,
    video_codec,
    audio_codec,
    status,
    created_at,
    updated_at
)
values (
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8,
    ?9,
    'pending',
    ?10,
    ?10
)
",
        params![
            task_id,
            draft.stream_key,
            draft.format_id,
            draft.media_type.as_str(),
            draft.extension,
            draft.width,
            draft.height,
            draft.video_codec,
            draft.audio_codec,
            draft.created_at,
        ],
    )?;
    Ok(transaction.last_insert_rowid())
}

/// 在独立事务中写入一个新下载流，并由外键校验所属任务。
pub(crate) fn create_download_stream(
    connection: &mut Connection,
    task_id: i64,
    draft: &DownloadTaskStreamDraft,
) -> Result<DownloadTaskStream, StorageError> {
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let task = get_download_task(&transaction, task_id)?.ok_or(StorageError::DownloadNotFound(task_id))?;
    if !task.task.status.can_accept_progress()
        || task.task.status == super::super::download::DownloadTaskStatus::Merging
    {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }
    if let Some(snapshot) = super::read::load_download_execution_snapshot(&transaction, task_id)? {
        snapshot.validate_stream(draft)?;
    }
    let id = insert_download_stream(&transaction, task_id, draft).map_err(map_download_write_error)?;
    transaction.commit().map_err(StorageError::Write)?;
    get_download_stream(connection, id)?.ok_or(StorageError::DownloadNotFound(task_id))
}
