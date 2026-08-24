use rusqlite::{params, Connection, OptionalExtension};

use super::super::config::EnvironmentConfig;
use super::super::error::StorageError;

/// 读取 config 表的唯一环境配置记录。
pub(crate) fn read_configuration(connection: &Connection) -> Result<Option<EnvironmentConfig>, StorageError> {
    connection
        .query_row(
            "
select
    version,
    yt_dlp_path,
    ffmpeg_path,
    default_download_path,
    theme,
    language,
    concurrent_downloads,
    proxy
from
    config
where
    singleton = 1
",
            [],
            EnvironmentConfig::from_row,
        )
        .optional()
        .map_err(StorageError::Read)
}

/// 原子保存完整配置快照；事务提交成功后调用方才更新内存快照。
pub(crate) fn save_configuration(
    connection: &mut Connection,
    configuration: &EnvironmentConfig,
) -> Result<(), StorageError> {
    connection
        .execute(
            "
insert into config (
    singleton,
    version,
    yt_dlp_path,
    ffmpeg_path,
    default_download_path,
    theme,
    language,
    concurrent_downloads,
    proxy
)
values (
    1,
    ?1,
    ?2,
    ?3,
    ?4,
    ?5,
    ?6,
    ?7,
    ?8
)
on conflict (singleton) do update set
    version = excluded.version,
    yt_dlp_path = excluded.yt_dlp_path,
    ffmpeg_path = excluded.ffmpeg_path,
    default_download_path = excluded.default_download_path,
    theme = excluded.theme,
    language = excluded.language,
    concurrent_downloads = excluded.concurrent_downloads,
    proxy = excluded.proxy
",
            params![
                configuration.version,
                configuration.yt_dlp_path,
                configuration.ffmpeg_path,
                configuration.default_download_path,
                configuration.theme,
                configuration.language,
                configuration.concurrent_downloads,
                configuration.proxy,
            ],
        )
        .map_err(StorageError::Write)?;
    Ok(())
}
