use super::state::AppSettings;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    None,
    SurroundingWhitespace,
    InvalidExecutablePath,
    ExecutableProbeFailed,
    InvalidDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsValidation {
    pub yt_dlp_error: ValidationError,
    pub ffmpeg_error: ValidationError,
    pub download_directory_error: ValidationError,
    pub proxy_error: ValidationError,
}

impl SettingsValidation {
    pub fn is_valid(self) -> bool {
        self.first_invalid_field().is_none()
    }

    pub fn first_invalid_field(self) -> Option<i32> {
        [
            self.yt_dlp_error,
            self.ffmpeg_error,
            self.download_directory_error,
            self.proxy_error,
        ]
        .into_iter()
        .position(|error| error != ValidationError::None)
        .map(|field| field as i32)
    }
}

pub fn validate_settings(settings: &AppSettings) -> SettingsValidation {
    // Executable probes may take noticeably longer than the pure value checks, so run them together.
    let (yt_dlp_error, ffmpeg_error) = std::thread::scope(|scope| {
        let yt_dlp = scope.spawn(|| validate_yt_dlp(&settings.yt_dlp_path));
        let ffmpeg = scope.spawn(|| validate_ffmpeg(&settings.ffmpeg_path));
        (
            yt_dlp
                .join()
                .unwrap_or(ValidationError::ExecutableProbeFailed),
            ffmpeg
                .join()
                .unwrap_or(ValidationError::ExecutableProbeFailed),
        )
    });

    SettingsValidation {
        yt_dlp_error,
        ffmpeg_error,
        download_directory_error: validate_download_directory(&settings.default_download_directory),
        proxy_error: if has_surrounding_whitespace(&settings.proxy) {
            ValidationError::SurroundingWhitespace
        } else {
            ValidationError::None
        },
    }
}

fn validate_yt_dlp(value: &str) -> ValidationError {
    if has_surrounding_whitespace(value) {
        return ValidationError::SurroundingWhitespace;
    }
    if value.is_empty() {
        return if command_succeeds(Path::new("yt-dlp"), "--version") {
            ValidationError::None
        } else {
            ValidationError::ExecutableProbeFailed
        };
    }

    let path = Path::new(value);
    validate_executable(path, "--version")
}

fn validate_ffmpeg(value: &str) -> ValidationError {
    if has_surrounding_whitespace(value) {
        return ValidationError::SurroundingWhitespace;
    }
    if value.is_empty() {
        return ValidationError::None;
    }

    let path = resolve_ffmpeg_path(Path::new(value));
    validate_executable(&path, "-version")
}

fn validate_executable(path: &Path, version_argument: &str) -> ValidationError {
    if !path.is_file() {
        ValidationError::InvalidExecutablePath
    } else if command_succeeds(path, version_argument) {
        ValidationError::None
    } else {
        ValidationError::ExecutableProbeFailed
    }
}

fn resolve_ffmpeg_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        })
    } else {
        path.to_path_buf()
    }
}

fn validate_download_directory(value: &str) -> ValidationError {
    if has_surrounding_whitespace(value) {
        ValidationError::SurroundingWhitespace
    } else if value.is_empty() || !Path::new(value).is_dir() {
        ValidationError::InvalidDirectory
    } else {
        ValidationError::None
    }
}

fn has_surrounding_whitespace(value: &str) -> bool {
    value != value.trim()
}

fn command_succeeds(program: &Path, version_argument: &str) -> bool {
    let mut child = match Command::new(program)
        .arg(version_argument)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let deadline = Instant::now() + COMMAND_TIMEOUT;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ffmpeg_path_is_valid_but_whitespace_is_not() {
        assert_eq!(validate_ffmpeg(""), ValidationError::None);
        assert_eq!(
            validate_ffmpeg("  "),
            ValidationError::SurroundingWhitespace
        );
    }

    #[test]
    fn surrounding_whitespace_takes_priority() {
        assert_eq!(
            validate_yt_dlp(" missing.exe"),
            ValidationError::SurroundingWhitespace
        );
        assert_eq!(
            validate_ffmpeg("missing.exe "),
            ValidationError::SurroundingWhitespace
        );
        assert_eq!(
            validate_download_directory(" C:/downloads"),
            ValidationError::SurroundingWhitespace
        );
    }

    #[test]
    fn rejects_missing_executable_and_download_directory() {
        assert_eq!(
            validate_yt_dlp("this-executable-should-not-exist.exe"),
            ValidationError::InvalidExecutablePath
        );
        assert_eq!(
            validate_ffmpeg("this-executable-should-not-exist.exe"),
            ValidationError::InvalidExecutablePath
        );
        assert_eq!(
            validate_download_directory("this-directory-should-not-exist-for-settings-validation"),
            ValidationError::InvalidDirectory
        );
    }

    #[test]
    fn proxy_accepts_empty_value_but_rejects_surrounding_whitespace() {
        let mut settings = AppSettings::default();
        assert_eq!(
            validate_settings(&settings).proxy_error,
            ValidationError::None
        );
        settings.proxy = " http://proxy.example ".into();
        assert_eq!(
            validate_settings(&settings).proxy_error,
            ValidationError::SurroundingWhitespace
        );
    }
}
