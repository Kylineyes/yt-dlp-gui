use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    YtDlp(ytd_rs::error::YtDlpError),
    NotFound(String),
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "File operation failed: {error}"),
            Self::Sqlite(error) => write!(formatter, "SQLite operation failed: {error}"),
            Self::YtDlp(error) => write!(formatter, "yt-dlp execution failed: {error}"),
            Self::NotFound(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for AppError {}

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
