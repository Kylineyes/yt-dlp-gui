use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

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

/// 更新字符串字段；field 只允许由本模块内部传入，避免把外部输入拼接进 SQL。
pub(super) fn update_text(connection: &Connection, field: &str, value: &str) -> Result<(), StorageError> {
    let changed = connection
        .execute(&format!("UPDATE config SET {field} = ?1"), [value])
        .map_err(StorageError::Write)?;
    if changed != 1 {
        return Err(StorageError::ConfigurationMissing);
    }
    Ok(())
}

/// 更新整数配置字段，并确保确实命中了唯一的配置记录。
pub(super) fn update_integer(connection: &Connection, field: &str, value: i8) -> Result<(), StorageError> {
    let changed = connection
        .execute(&format!("UPDATE config SET {field} = ?1"), [value])
        .map_err(StorageError::Write)?;
    if changed != 1 {
        return Err(StorageError::ConfigurationMissing);
    }
    Ok(())
}
