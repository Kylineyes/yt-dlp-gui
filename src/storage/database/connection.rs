use std::path::Path;

use rusqlite::Connection;

use super::super::error::StorageError;

/// 打开数据库并启用当前连接的外键约束。
pub(crate) fn open_database(database_path: &Path) -> Result<Connection, StorageError> {
    let connection = Connection::open(database_path).map_err(StorageError::Open)?;
    connection
        .execute_batch(
            "
pragma foreign_keys = on;
",
        )
        .map_err(StorageError::Open)?;
    let enabled: i64 = connection
        .query_row(
            "
pragma foreign_keys
",
            [],
            |row| row.get(0),
        )
        .map_err(StorageError::Read)?;
    if enabled != 1 {
        return Err(StorageError::Write(rusqlite::Error::InvalidQuery));
    }
    Ok(connection)
}
