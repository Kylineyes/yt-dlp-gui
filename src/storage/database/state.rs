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
    if now < 0 {
        return Err(StorageError::InvalidDownloadInput);
    }
    if status == DownloadTaskStatus::Paused {
        return Err(StorageError::InvalidDownloadStatusTransition);
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
    if !current.can_transition_to(status)
        || (current == DownloadTaskStatus::Paused && status == DownloadTaskStatus::Preparing)
    {
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

/// 原子暂停准备中或下载中的任务及其所有尚未完成的流。
pub(crate) fn pause_download_task(connection: &mut Connection, id: i64, now: i64) -> Result<(), StorageError> {
    if now < 0 {
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
    if !matches!(
        DownloadTaskStatus::parse(current)?,
        DownloadTaskStatus::Preparing | DownloadTaskStatus::Downloading
    ) {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }

    transaction
        .execute(
            "
update
    download_tasks
set
    status = 'paused',
    speed_bytes_per_second = null,
    elapsed_seconds = null,
    eta_seconds = null,
    updated_at = ?1
where
    id = ?2
",
            params![now, id],
        )
        .map_err(StorageError::Write)?;
    transaction
        .execute(
            "
update
    download_task_streams
set
    status = 'paused',
    speed_bytes_per_second = null,
    elapsed_seconds = null,
    eta_seconds = null,
    updated_at = ?1
where
    task_id = ?2
    and status in ('pending', 'preparing', 'downloading')
",
            params![now, id],
        )
        .map_err(StorageError::Write)?;
    transaction.commit().map_err(StorageError::Write)
}

/// 原子准备已暂停的任务继续执行，保持累计进度和首次开始时间。
pub(crate) fn prepare_resumed_download(connection: &mut Connection, id: i64, now: i64) -> Result<(), StorageError> {
    if now < 0 {
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
    if DownloadTaskStatus::parse(current)? != DownloadTaskStatus::Paused {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }
    super::read::load_download_execution_snapshot(&transaction, id)?
        .ok_or(StorageError::DownloadExecutionSnapshotMissing(id))?;

    transaction
        .execute(
            "
update
    download_tasks
set
    status = 'preparing',
    updated_at = ?1
where
    id = ?2
",
            params![now, id],
        )
        .map_err(StorageError::Write)?;
    transaction
        .execute(
            "
update
    download_task_streams
set
    status = 'preparing',
    updated_at = ?1
where
    task_id = ?2
    and status = 'paused'
",
            params![now, id],
        )
        .map_err(StorageError::Write)?;
    transaction.commit().map_err(StorageError::Write)
}

/// 将异常中断留下的活动任务统一转换为暂停状态。
pub(crate) fn recover_interrupted_downloads(connection: &mut Connection, now: i64) -> Result<usize, StorageError> {
    if now < 0 {
        return Err(StorageError::InvalidDownloadInput);
    }

    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let mut statement = transaction
        .prepare(
            "
select
    id
from
    download_tasks
where
    status in ('preparing', 'downloading', 'merging')
order by
    id
",
        )
        .map_err(StorageError::Read)?;
    let task_ids = statement
        .query_map([], |row| row.get::<_, i64>(0))
        .map_err(StorageError::Read)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::Read)?;
    drop(statement);

    for id in &task_ids {
        transaction
            .execute(
                "
update
    download_tasks
set
    status = 'paused',
    speed_bytes_per_second = null,
    elapsed_seconds = null,
    eta_seconds = null,
    updated_at = ?1
where
    id = ?2
",
                params![now, id],
            )
            .map_err(StorageError::Write)?;
        transaction
            .execute(
                "
update
    download_task_streams
set
    status = 'paused',
    speed_bytes_per_second = null,
    elapsed_seconds = null,
    eta_seconds = null,
    updated_at = ?1
where
    task_id = ?2
    and status in ('pending', 'preparing', 'downloading', 'merging')
",
                params![now, id],
            )
            .map_err(StorageError::Write)?;
    }
    transaction.commit().map_err(StorageError::Write)?;
    Ok(task_ids.len())
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

pub(crate) fn delete_download_tasks(connection: &mut Connection, ids: &[i64]) -> Result<(), StorageError> {
    if ids.is_empty() {
        return Ok(());
    }

    let transaction = connection.transaction().map_err(StorageError::Write)?;
    for id in ids {
        transaction
            .execute(
                "
delete from
    download_tasks
where
    id = ?1
",
                [id],
            )
            .map_err(StorageError::Write)?;
    }
    transaction.commit().map_err(StorageError::Write)
}
/// 在事务内按流状态机更新状态和生命周期时间戳。
pub(crate) fn update_download_stream_status(
    connection: &mut Connection,
    stream_id: i64,
    status: DownloadTaskStatus,
    now: i64,
) -> Result<(), StorageError> {
    if now < 0 {
        return Err(StorageError::InvalidDownloadInput);
    }
    if status == DownloadTaskStatus::Paused {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }

    let transaction = connection.transaction().map_err(StorageError::Write)?;
    let (current, parent): (String, String) = transaction
        .query_row(
            "
select
    download_task_streams.status,
    download_tasks.status
from
    download_task_streams
join
    download_tasks on download_tasks.id = download_task_streams.task_id
where
    download_task_streams.id = ?1
",
            [stream_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(StorageError::Read)?
        .ok_or(StorageError::DownloadStreamNotFound(stream_id))?;
    let parent = DownloadTaskStatus::parse(parent)?;
    if !parent.can_accept_progress() && !matches!(status, DownloadTaskStatus::Cancelled | DownloadTaskStatus::Failed) {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }
    let current = DownloadTaskStatus::parse(current)?;
    if !current.can_stream_transition_to(status)
        || (current == DownloadTaskStatus::Paused && status == DownloadTaskStatus::Preparing)
    {
        return Err(StorageError::InvalidDownloadStatusTransition);
    }

    let started_at = matches!(status, DownloadTaskStatus::Downloading).then_some(now);
    let finished_at = status.is_terminal().then_some(now);
    transaction
        .execute(
            "
update
    download_task_streams
set
    status = ?1,
    started_at = coalesce(started_at, ?2),
    finished_at = ?3,
    updated_at = ?4
where
    id = ?5
",
            params![status.as_str(), started_at, finished_at, now, stream_id],
        )
        .map_err(StorageError::Write)?;
    transaction.commit().map_err(StorageError::Write)
}

/// 将下载流标记为完成，不隐式修改其进度字段。
pub(crate) fn complete_download_stream(
    connection: &mut Connection,
    stream_id: i64,
    finished_at: i64,
) -> Result<(), StorageError> {
    update_download_stream_status(connection, stream_id, DownloadTaskStatus::Completed, finished_at)
}

/// 将下载流标记为失败；错误摘要继续由所属任务记录。
pub(crate) fn fail_download_stream(
    connection: &mut Connection,
    stream_id: i64,
    finished_at: i64,
) -> Result<(), StorageError> {
    update_download_stream_status(connection, stream_id, DownloadTaskStatus::Failed, finished_at)
}

/// 将下载流标记为取消并记录终态时间。
pub(crate) fn cancel_download_stream(
    connection: &mut Connection,
    stream_id: i64,
    finished_at: i64,
) -> Result<(), StorageError> {
    update_download_stream_status(connection, stream_id, DownloadTaskStatus::Cancelled, finished_at)
}
