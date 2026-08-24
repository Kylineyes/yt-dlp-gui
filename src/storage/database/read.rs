use rusqlite::{params, Connection, OptionalExtension};

use super::super::download::{DownloadTask, DownloadTaskFilter, DownloadTaskStream, DownloadTaskWithStreams};
use super::super::error::StorageError;
use super::support::{map_stream, map_task, STREAM_SELECT, TASK_SELECT};

const TASK_BY_ID_SUFFIX: &str = r#"
where
    id = ?1
"#;

const STREAMS_BY_TASK_SUFFIX: &str = r#"
where
    task_id = ?1
order by
    id
"#;

const STREAM_BY_ID_SUFFIX: &str = r#"
where
    id = ?1
"#;

const TASK_LIST_WITH_STATUS_SUFFIX: &str = r#"
where
    status = ?1
order by
    updated_at desc,
    id desc
limit
    ?2
"#;

const TASK_LIST_SUFFIX: &str = r#"
order by
    updated_at desc,
    id desc
limit
    ?1
"#;

/// 读取任务及其关联流，列映射集中在 support 模块维护。
pub(crate) fn get_download_task(
    connection: &Connection,
    id: i64,
) -> Result<Option<DownloadTaskWithStreams>, StorageError> {
    let task_sql = format!("{TASK_SELECT}{TASK_BY_ID_SUFFIX}");
    let task = connection
        .query_row(&task_sql, [id], map_task)
        .optional()
        .map_err(StorageError::Read)?;
    let Some(task) = task else {
        return Ok(None);
    };

    let streams_sql = format!("{STREAM_SELECT}{STREAMS_BY_TASK_SUFFIX}");
    let mut statement = connection.prepare(&streams_sql).map_err(StorageError::Read)?;
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
    let limit = filter.limit.unwrap_or(100).min(1000) as i64;
    let sql = if filter.status.is_some() {
        format!("{TASK_SELECT}{TASK_LIST_WITH_STATUS_SUFFIX}")
    } else {
        format!("{TASK_SELECT}{TASK_LIST_SUFFIX}")
    };
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
    let sql = format!("{STREAM_SELECT}{STREAM_BY_ID_SUFFIX}");
    connection
        .query_row(&sql, [id], map_stream)
        .optional()
        .map_err(StorageError::Read)
}
