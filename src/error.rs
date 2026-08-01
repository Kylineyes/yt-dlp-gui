use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageStage {
    ResolveExecutablePath,
    CreateDatabaseDirectory,
    OpenDatabase,
    ConfigureConnection,
    CreateTables,
    MigrateSchema,
}

impl Display for StorageStage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            Self::ResolveExecutablePath => "resolve the executable directory",
            Self::CreateDatabaseDirectory => "create the database directory",
            Self::OpenDatabase => "open or create the SQLite database",
            Self::ConfigureConnection => "configure the SQLite connection",
            Self::CreateTables => "create the SQLite tables",
            Self::MigrateSchema => "migrate the SQLite schema",
        };
        formatter.write_str(description)
    }
}

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    StorageIo {
        stage: StorageStage,
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    StorageSqlite {
        stage: StorageStage,
        path: PathBuf,
        source: rusqlite::Error,
    },
    YtDlp(ytd_rs::error::YtDlpError),
    NotFound(String),
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "File operation failed: {error}"),
            Self::Sqlite(error) => write!(formatter, "SQLite operation failed: {error}"),
            Self::StorageIo {
                stage,
                path: Some(path),
                source,
            } => write!(
                formatter,
                "Failed to {stage} at '{}': {source}",
                path.display()
            ),
            Self::StorageIo {
                stage,
                path: None,
                source,
            } => write!(formatter, "Failed to {stage}: {source}"),
            Self::StorageSqlite {
                stage,
                path,
                source,
            } => write!(
                formatter,
                "Failed to {stage} at '{}': {source}",
                path.display()
            ),
            Self::YtDlp(error) => write!(formatter, "yt-dlp execution failed: {error}"),
            Self::NotFound(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sqlite(error) => Some(error),
            Self::StorageIo { source, .. } => Some(source),
            Self::StorageSqlite { source, .. } => Some(source),
            Self::YtDlp(error) => Some(error),
            Self::NotFound(_) => None,
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<ytd_rs::error::YtDlpError> for AppError {
    fn from(error: ytd_rs::error::YtDlpError) -> Self {
        Self::YtDlp(error)
    }
}
