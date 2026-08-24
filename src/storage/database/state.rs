use rusqlite::{params, Connection, OptionalExtension};

use super::super::download::DownloadTaskStatus;
use super::super::error::StorageError;
use super::support::map_download_write_error;

/// 在事务内校验并更新任务状态，避免终态竞争产生双重成功。
pub(crate) fn update_download_status(
    connection: &mut Connection,
    id: i64,
    status: DownloadTaskStatus,
    now: i64,
) -> Result<(), StorageError> {
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let current = transaction
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
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::Read)?
        .ok_or(StorageError::DownloadNotFound(id))?;
    let current = DownloadTaskStatus::parse(current)?;
    if !current.can_transition_to(status) {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }
    let started = matches!(status, DownloadTaskStatus::Downloading).then_some(now);
    let finished = status.is_terminal().then_some(now);
    transaction
        .execute(
            "
update
    download_tasks
set
    status = ?1,
    started_at = coalesce(started_at, ?2),
    finished_at = ?3,
    updated_at = ?4
where
    id = ?5
",
            params![status.as_str(), started, finished, now, id],
        )
        .map_err(StorageError::Write)?;
    transaction.commit().map_err(StorageError::Write)
}

/// 以完成状态结束任务，并保存最终输出路径。
pub(crate) fn complete_download_task(
    connection: &mut Connection,
    id: i64,
    output_path: String,
    finished_at: i64,
) -> Result<(), StorageError> {
    finish_download_task(
        connection,
        id,
        DownloadTaskStatus::Completed,
        finished_at,
        Some(output_path),
        None,
        None,
    )
}

/// 以失败状态结束任务，并保存错误码和错误摘要。
pub(crate) fn fail_download_task(
    connection: &mut Connection,
    id: i64,
    error_code: Option<String>,
    error_message: String,
    finished_at: i64,
) -> Result<(), StorageError> {
    finish_download_task(
        connection,
        id,
        DownloadTaskStatus::Failed,
        finished_at,
        None,
        error_code,
        Some(error_message),
    )
}

/// 以取消状态结束任务，并记录终态时间。
pub(crate) fn cancel_download_task(connection: &mut Connection, id: i64, finished_at: i64) -> Result<(), StorageError> {
    finish_download_task(
        connection,
        id,
        DownloadTaskStatus::Cancelled,
        finished_at,
        None,
        None,
        None,
    )
}

fn finish_download_task(
    connection: &mut Connection,
    id: i64,
    target_status: DownloadTaskStatus,
    finished_at: i64,
    output_path: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
) -> Result<(), StorageError> {
    if finished_at < 0 || (target_status == DownloadTaskStatus::Failed && error_message.is_none()) {
        return Err(StorageError::InvalidDownloadInput);
    }
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let current = transaction
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
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::Read)?
        .ok_or(StorageError::DownloadNotFound(id))?;
    let current = DownloadTaskStatus::parse(current)?;
    if !current.can_transition_to(target_status) {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }
    transaction
        .execute(
            "
update
    download_tasks
set
    status = ?1,
    output_path = coalesce(?2, output_path),
    error_code = ?3,
    error_message = ?4,
    finished_at = ?5,
    updated_at = ?5
where
    id = ?6
",
            params![
                target_status.as_str(),
                output_path,
                error_code,
                error_message,
                finished_at,
                id
            ],
        )
        .map_err(map_download_write_error)?;
    transaction.commit().map_err(StorageError::Write)
}
