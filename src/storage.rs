mod app_config;
mod config_keys;
mod downloads;
mod initialization;
mod migrations;
mod schema;

use crate::error::{AppError, StorageStage};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DATABASE_FILE_NAME: &str = "application.sqlite3";

pub struct Storage {
    connection: Connection,
}

fn database_path_next_to_executable(executable_path: &Path) -> Result<PathBuf, AppError> {
    let executable_directory = executable_path
        .parent()
        .ok_or_else(|| AppError::StorageIo {
            stage: StorageStage::ResolveExecutablePath,
            path: Some(executable_path.to_path_buf()),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Executable path has no parent directory",
            ),
        })?;
    Ok(executable_directory.join(DATABASE_FILE_NAME))
}

fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
