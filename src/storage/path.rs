use std::env;
use std::path::PathBuf;

use super::error::StorageError;

/// 未传入 `-c` 时使用的数据库文件名。
pub(super) const DEFAULT_DATABASE_FILE: &str = "app.sqlite";

/// 解析启动参数；仅识别 `-c`，其他参数交由后续模块处理。
pub(super) fn database_path_from_args() -> Result<PathBuf, StorageError> {
    let mut args = env::args_os().skip(1);
    while let Some(argument) = args.next() {
        if argument == "-c" {
            return args.next().map(PathBuf::from).ok_or(StorageError::MissingDatabasePath);
        }
    }

    let executable = env::current_exe().map_err(StorageError::ExecutablePath)?;
    let directory = executable
        .parent()
        .ok_or_else(|| StorageError::InvalidDatabasePath(executable.clone()))?;
    Ok(directory.join(DEFAULT_DATABASE_FILE))
}
