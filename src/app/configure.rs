use std::env;
use std::path::{Path, PathBuf};

use crate::storage::{EnvironmentConfig, CONFIG_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigureField {
    YtDlpPath,
    FfmpegPath,
    DefaultDownloadPath,
    Proxy,
    ConcurrentDownloads,
    Language,
    Theme,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigureError {
    EmptyRequiredPath,
    InvalidPath(ConfigureField),
    MissingFile(ConfigureField),
    NotAFile(ConfigureField),
    InvalidToolName(ConfigureField),
    InvalidToolExtension(ConfigureField),
    MissingDirectory,
    NotADirectory,
    HasLeadingOrTrailingWhitespace,
    InvalidConcurrentDownloads,
    InvalidLanguage,
    InvalidTheme,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigureValidationError {
    pub field: ConfigureField,
    pub error: ConfigureError,
}

pub fn validate(configuration: &EnvironmentConfig) -> Result<(), ConfigureValidationError> {
    validate_tool_path(ConfigureField::YtDlpPath, &configuration.yt_dlp_path, "yt-dlp.exe")?;
    validate_tool_path(ConfigureField::FfmpegPath, &configuration.ffmpeg_path, "ffmpeg.exe")?;
    validate_download_path(&configuration.default_download_path)?;
    validate_proxy(&configuration.proxy)?;
    validate_concurrent_downloads(configuration.concurrent_downloads)?;
    validate_language(&configuration.language)?;
    validate_theme(&configuration.theme)
}

pub fn normalize_draft(mut configuration: EnvironmentConfig) -> EnvironmentConfig {
    configuration.version = CONFIG_VERSION.to_string();
    configuration
}

pub fn find_on_path(executable_name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(executable_name))
        .find(|candidate| candidate.is_file())
}

fn validate_tool_path(field: ConfigureField, value: &str, expected_name: &str) -> Result<(), ConfigureValidationError> {
    if value.is_empty() {
        return Err(ConfigureValidationError {
            field,
            error: ConfigureError::EmptyRequiredPath,
        });
    }
    reject_whitespace(field, value)?;
    let path = Path::new(value);
    if path.as_os_str().is_empty() || path.components().next().is_none() {
        return Err(ConfigureValidationError {
            field,
            error: ConfigureError::InvalidPath(field),
        });
    }
    if !path.exists() {
        return Err(ConfigureValidationError {
            field,
            error: ConfigureError::MissingFile(field),
        });
    }
    if !path.is_file() {
        return Err(ConfigureValidationError {
            field,
            error: ConfigureError::NotAFile(field),
        });
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| !extension.eq_ignore_ascii_case("exe"))
        .unwrap_or(true)
    {
        return Err(ConfigureValidationError {
            field,
            error: ConfigureError::InvalidToolExtension(field),
        });
    }
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| !name.eq_ignore_ascii_case(expected_name))
        .unwrap_or(true)
    {
        return Err(ConfigureValidationError {
            field,
            error: ConfigureError::InvalidToolName(field),
        });
    }
    Ok(())
}

fn validate_download_path(value: &str) -> Result<(), ConfigureValidationError> {
    if value.is_empty() {
        return Ok(());
    }
    reject_whitespace(ConfigureField::DefaultDownloadPath, value)?;
    let path = Path::new(value);
    if path.components().next().is_none() {
        return Err(ConfigureValidationError {
            field: ConfigureField::DefaultDownloadPath,
            error: ConfigureError::InvalidPath(ConfigureField::DefaultDownloadPath),
        });
    }
    if !path.exists() {
        return Err(ConfigureValidationError {
            field: ConfigureField::DefaultDownloadPath,
            error: ConfigureError::MissingDirectory,
        });
    }
    if !path.is_dir() {
        return Err(ConfigureValidationError {
            field: ConfigureField::DefaultDownloadPath,
            error: ConfigureError::NotADirectory,
        });
    }
    Ok(())
}

fn validate_proxy(value: &str) -> Result<(), ConfigureValidationError> {
    if value.trim() != value {
        return Err(ConfigureValidationError {
            field: ConfigureField::Proxy,
            error: ConfigureError::HasLeadingOrTrailingWhitespace,
        });
    }
    Ok(())
}

fn validate_concurrent_downloads(value: i8) -> Result<(), ConfigureValidationError> {
    if (0..=16).contains(&value) {
        Ok(())
    } else {
        Err(ConfigureValidationError {
            field: ConfigureField::ConcurrentDownloads,
            error: ConfigureError::InvalidConcurrentDownloads,
        })
    }
}

fn validate_language(value: &str) -> Result<(), ConfigureValidationError> {
    if matches!(value, "zh-CN" | "en-US") {
        Ok(())
    } else {
        Err(ConfigureValidationError {
            field: ConfigureField::Language,
            error: ConfigureError::InvalidLanguage,
        })
    }
}

fn validate_theme(value: &str) -> Result<(), ConfigureValidationError> {
    if matches!(value, "system" | "light" | "dark") {
        Ok(())
    } else {
        Err(ConfigureValidationError {
            field: ConfigureField::Theme,
            error: ConfigureError::InvalidTheme,
        })
    }
}

fn reject_whitespace(field: ConfigureField, value: &str) -> Result<(), ConfigureValidationError> {
    if value.trim() == value {
        Ok(())
    } else {
        Err(ConfigureValidationError {
            field,
            error: ConfigureError::HasLeadingOrTrailingWhitespace,
        })
    }
}

pub mod picker {
    use std::path::PathBuf;

    pub fn choose_executable() -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Executable files", &["exe"])
            .pick_file()
    }

    pub fn choose_directory() -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }
}
