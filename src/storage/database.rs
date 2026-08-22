use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use super::config::EnvironmentConfig;
use super::error::StorageError;

/// 以读写模式打开既有数据库，禁止 SQLite 在路径不存在时自动创建文件。
pub(super) fn open_existing_database(database_path: &Path) -> Result<Connection, StorageError> {
    if !database_path.is_file() {
        return Err(StorageError::DatabaseFileUnavailable(database_path.to_path_buf()));
    }
    Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE).map_err(StorageError::Open)
}

/// 读取 config 表的第一条记录；当前版本要求该表只保存一条环境配置。
pub(super) fn read_configuration(connection: &Connection) -> Result<EnvironmentConfig, StorageError> {
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
        .map_err(StorageError::Read)?
        .ok_or(StorageError::ConfigurationMissing)
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
