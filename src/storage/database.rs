use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

use super::config::EnvironmentConfig;
use super::error::StorageError;

/// 打开数据库文件；首次启动时允许 SQLite 创建目标文件。
pub(super) fn open_database(database_path: &Path) -> Result<Connection, StorageError> {
    Connection::open(database_path).map_err(StorageError::Open)
}

/// 读取 config 表的第一条记录；当前版本要求该表只保存一条环境配置。
pub(super) fn read_configuration(connection: &Connection) -> Result<Option<EnvironmentConfig>, StorageError> {
    connection
        .query_row(
            concat!(
                "SELECT version, yt_dlp_path, ffmpeg_path, default_download_path, ",
                "theme, language, concurrent_downloads, proxy ",
                "FROM config LIMIT 1"
            ),
            [],
            EnvironmentConfig::from_row,
        )
        .optional()
        .map_err(StorageError::Read)
}

/// 原子保存完整配置快照；事务失败时既有记录和内存快照都保持不变。
pub(super) fn save_configuration(
    connection: &mut Connection,
    configuration: &EnvironmentConfig,
) -> Result<(), StorageError> {
    let transaction = connection.transaction().map_err(StorageError::Write)?;
    transaction
        .execute("DELETE FROM config", [])
        .map_err(StorageError::Write)?;
    transaction
        .execute(
            concat!(
                "INSERT INTO config (version, yt_dlp_path, ffmpeg_path, ",
                "default_download_path, theme, language, concurrent_downloads, proxy) ",
                "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
            ),
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
    transaction.commit().map_err(StorageError::Write)
}
