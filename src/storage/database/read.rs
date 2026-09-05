use rusqlite::{params, Connection, OptionalExtension};

use super::super::download::{
    DownloadExecutionOptions, DownloadExecutionSnapshot, DownloadTask, DownloadTaskFilter, DownloadTaskStream,
    DownloadTaskWithStreams,
};
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

pub(crate) fn load_download_execution_snapshot(
    connection: &Connection,
    task_id: i64,
) -> Result<Option<DownloadExecutionSnapshot>, StorageError> {
    let task_exists: bool = connection
        .query_row(
            "
select
    exists (
        select
            1
        from
            download_tasks
        where
            id = ?1
    )
",
            [task_id],
            |row| row.get(0),
        )
        .map_err(StorageError::Read)?;
    if !task_exists {
        return Err(StorageError::DownloadNotFound(task_id));
    }

    connection
        .query_row(
            "
select
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
from
    download_task_execution_snapshots
where
    task_id = ?1
",
            [task_id],
            |row| {
                Ok(DownloadExecutionSnapshot {
                    source_url: row.get(0)?,
                    video_format_id: row.get(1)?,
                    audio_format_id: row.get(2)?,
                    output_template: row.get(3)?,
                    target_directory: row.get(4)?,
                    temporary_directory: row.get(5)?,
                    merge_output_format: row.get(6)?,
                    options: DownloadExecutionOptions {
                        rate_limit: row.get(7)?,
                        retries: row.get(8)?,
                        fragment_retries: row.get(9)?,
                        file_access_retries: row.get(10)?,
                        concurrent_fragments: row.get(11)?,
                    },
                })
            },
        )
        .optional()
        .map_err(StorageError::Read)
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
