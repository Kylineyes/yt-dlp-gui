use std::env;
use std::path::PathBuf;

use super::error::StorageError;

/// 未传入 `-c` 时使用的数据库文件名。
pub(super) const DEFAULT_DATABASE_FILE: &str = "app.sqlite";

/// 将 CLI 提供的可选路径解析为最终数据库路径。
///
/// 参数语法由 `crate::cli` 统一负责；此处只处理显式路径和默认路径。
pub(super) fn resolve_database_path(config_path: Option<PathBuf>) -> Result<PathBuf, StorageError> {
    config_path.map_or_else(default_database_path, Ok)
}

/// 将数据库放在当前可执行文件所在目录，避免依赖进程当前工作目录。
fn default_database_path() -> Result<PathBuf, StorageError> {
    let executable = env::current_exe().map_err(StorageError::ExecutablePath)?;
    let directory = executable
        .parent()
        .ok_or_else(|| StorageError::InvalidDatabasePath(executable.clone()))?;
    Ok(directory.join(DEFAULT_DATABASE_FILE))
}
