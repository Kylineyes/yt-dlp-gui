use super::state::AppSettings;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsValidation {
    pub yt_dlp_path: bool,
    pub ffmpeg_path: bool,
    pub default_download_directory: bool,
}

pub fn validate_settings(settings: &AppSettings) -> SettingsValidation {
    // Executable probes may take noticeably longer than the pure value checks, so run them together.
    let (yt_dlp_path, ffmpeg_path) = std::thread::scope(|scope| {
        let yt_dlp = scope.spawn(|| validate_yt_dlp(&settings.yt_dlp_path));
        let ffmpeg = scope.spawn(|| validate_ffmpeg(&settings.ffmpeg_path));
        (
            yt_dlp.join().unwrap_or(false),
            ffmpeg.join().unwrap_or(false),
        )
    });

    SettingsValidation {
        yt_dlp_path,
        ffmpeg_path,
        default_download_directory: validate_download_directory(
            &settings.default_download_directory,
        ),
    }
}

fn validate_yt_dlp(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return command_succeeds(Path::new("yt-dlp"), "--version");
    }

    let path = Path::new(value);
    path.is_file() && command_succeeds(path, "--version")
}

fn validate_ffmpeg(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return true;
    }

    let path = resolve_ffmpeg_path(Path::new(value));
    path.is_file() && command_succeeds(&path, "-version")
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

fn validate_download_directory(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && Path::new(value).is_dir()
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
    fn empty_ffmpeg_path_is_valid() {
        assert!(validate_ffmpeg("  "));
    }

    #[test]
    fn rejects_missing_download_directory() {
        assert!(!validate_download_directory(""));
        assert!(!validate_download_directory(
            "this-directory-should-not-exist-for-settings-validation"
        ));
    }
}
