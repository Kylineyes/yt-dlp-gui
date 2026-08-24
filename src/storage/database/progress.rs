use rusqlite::{params, Connection, OptionalExtension};

use super::super::download::{DownloadProgress, DownloadTaskStatus};
use super::super::error::StorageError;

/// 更新任务进度字段，终态任务不允许继续写入进度。
pub(crate) fn update_download_progress(
    connection: &mut Connection,
    id: i64,
    progress: DownloadProgress,
) -> Result<(), StorageError> {
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let status: String = transaction
        .query_row(
            "
select
    status
from
    download_tasks
where
    id = ?1
",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::Read)?
        .ok_or(StorageError::DownloadNotFound(id))?;
    if DownloadTaskStatus::parse(status)?.is_terminal() {
        return Err(StorageError::InvalidDownloadProgress);
    }
    transaction
        .execute(
            "
update
    download_tasks
set
    progress_percent = ?1,
    downloaded_bytes = ?2,
    total_bytes = ?3,
    total_bytes_estimate = ?4,
    speed_bytes_per_second = ?5,
    elapsed_seconds = ?6,
    eta_seconds = ?7,
    updated_at = ?8
where
    id = ?9
",
            params![
                progress.progress_percent.map(i64::from),
                progress.downloaded_bytes,
                progress.total_bytes,
                progress.total_bytes_estimate,
                progress.speed_bytes_per_second,
                progress.elapsed_seconds,
                progress.eta_seconds,
                progress.updated_at,
                id,
            ],
        )
        .map_err(StorageError::Write)?;
    transaction.commit().map_err(StorageError::Write)
}

/// 更新单个流的进度字段，不负责汇总任务级进度。
pub(crate) fn update_download_stream_progress(
    connection: &mut Connection,
    id: i64,
    progress: DownloadProgress,
) -> Result<(), StorageError> {
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let status: String = transaction
        .query_row(
            "
select
    status
from
    download_task_streams
where
    id = ?1
",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(StorageError::Read)?
        .ok_or(StorageError::DownloadNotFound(id))?;
    if DownloadTaskStatus::parse(status)?.is_terminal() {
        return Err(StorageError::InvalidDownloadProgress);
    }
    transaction
        .execute(
            "
update
    download_task_streams
set
    progress_percent = ?1,
    downloaded_bytes = ?2,
    total_bytes = ?3,
    total_bytes_estimate = ?4,
    speed_bytes_per_second = ?5,
    elapsed_seconds = ?6,
    eta_seconds = ?7,
    updated_at = ?8
where
    id = ?9
",
            params![
                progress.progress_percent.map(i64::from),
                progress.downloaded_bytes,
                progress.total_bytes,
                progress.total_bytes_estimate,
                progress.speed_bytes_per_second,
                progress.elapsed_seconds,
                progress.eta_seconds,
                progress.updated_at,
                id,
            ],
        )
        .map_err(StorageError::Write)?;
    transaction.commit().map_err(StorageError::Write)
}
