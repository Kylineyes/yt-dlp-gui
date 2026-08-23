use rusqlite::{params, Connection, OptionalExtension};

use super::super::download::{DownloadTask, DownloadTaskFilter, DownloadTaskStream, DownloadTaskWithStreams};
use super::super::error::StorageError;
use super::support::{map_stream, map_task, STREAM_SELECT, TASK_SELECT};

/// 读取任务及其关联流，列映射集中在 support 模块维护。
pub(crate) fn get_download_task(
    connection: &Connection,
    id: i64,
) -> Result<Option<DownloadTaskWithStreams>, StorageError> {
    let task = connection
        .query_row(TASK_SELECT, [id], map_task)
        .optional()
        .map_err(StorageError::Read)?;
    let Some(task) = task else { return Ok(None) };
    let mut statement = connection.prepare(STREAM_SELECT).map_err(StorageError::Read)?;
    let streams = statement
        .query_map([id], map_stream)
        .map_err(StorageError::Read)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StorageError::Read)?;
    Ok(Some(DownloadTaskWithStreams { task, streams }))
}

/// 按更新时间和 ID 稳定排序读取下载任务列表。
pub(crate) fn list_download_tasks(
    connection: &Connection,
    filter: DownloadTaskFilter,
) -> Result<Vec<DownloadTask>, StorageError> {
    let mut sql = String::from(TASK_SELECT);
    if filter.status.is_some() {
        sql.push_str(" WHERE status = ?1");
    }
    sql.push_str(if filter.status.is_some() {
        " ORDER BY updated_at DESC, id DESC LIMIT ?2"
    } else {
        " ORDER BY updated_at DESC, id DESC LIMIT ?1"
    });
    let limit = filter.limit.unwrap_or(100).min(1000) as i64;
    let mut statement = connection.prepare(&sql).map_err(StorageError::Read)?;
    let rows = match filter.status {
        Some(status) => statement.query_map(params![status.as_str(), limit], map_task),
        None => statement.query_map(params![limit], map_task),
    }
    .map_err(StorageError::Read)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(StorageError::Read)
}

pub(crate) fn get_download_stream(
    connection: &Connection,
    id: i64,
) -> Result<Option<DownloadTaskStream>, StorageError> {
    connection
        .query_row(STREAM_SELECT, [id], map_stream)
        .optional()
        .map_err(StorageError::Read)
}
