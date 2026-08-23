use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use yt_dlp_gui::app::configure::{find_on_path, validate, ConfigureError, ConfigureField};
use yt_dlp_gui::storage::EnvironmentConfig;

fn temporary_file(name: &str) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("yt-dlp-gui-{name}-{stamp}.exe"))
}

fn valid_configuration(path: &PathBuf) -> EnvironmentConfig {
    EnvironmentConfig {
        version: "0.0.1".to_string(),
        yt_dlp_path: path.display().to_string(),
        ffmpeg_path: path.display().to_string(),
        default_download_path: std::env::temp_dir().display().to_string(),
        theme: "system".to_string(),
        language: "en-US".to_string(),
        concurrent_downloads: 0,
        proxy: String::new(),
    }
}

#[test]
fn draft_defaults_match_configuration_contract() {
    let configuration = EnvironmentConfig::draft_default();
    assert_eq!(configuration.yt_dlp_path, "");
    assert_eq!(configuration.ffmpeg_path, "");
    assert_eq!(configuration.default_download_path, "");
    assert_eq!(configuration.proxy, "");
    assert_eq!(configuration.concurrent_downloads, 0);
    assert_eq!(configuration.language, "en-US");
    assert_eq!(configuration.theme, "system");
}

#[test]
fn validation_reports_fields_in_display_order() {
    let path = temporary_file("missing");
    let configuration = valid_configuration(&path);
    let error = validate(&configuration).unwrap_err();
    assert_eq!(error.field, ConfigureField::YtDlpPath);
    assert_eq!(error.error, ConfigureError::MissingFile(ConfigureField::YtDlpPath));
}

#[test]
fn validation_accepts_existing_tools_and_directory() {
    let directory = std::env::temp_dir().join(format!(
        "yt-dlp-gui-tools-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let yt_dlp_path = directory.join("yt-dlp.exe");
    let ffmpeg_path = directory.join("ffmpeg.exe");
    fs::write(&yt_dlp_path, b"test").unwrap();
    fs::write(&ffmpeg_path, b"test").unwrap();
    let mut configuration = valid_configuration(&yt_dlp_path);
    configuration.ffmpeg_path = ffmpeg_path.display().to_string();
    assert!(validate(&configuration).is_ok());
    fs::remove_file(yt_dlp_path).unwrap();
    fs::remove_file(ffmpeg_path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn validation_rejects_wrong_tool_name_on_yt_dlp_field() {
    let directory = std::env::temp_dir().join(format!(
        "yt-dlp-gui-tools-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let yt_dlp_path = directory.join("wrong-name.exe");
    let ffmpeg_path = directory.join("ffmpeg.exe");
    fs::write(&yt_dlp_path, b"test").unwrap();
    fs::write(&ffmpeg_path, b"test").unwrap();
    let mut configuration = valid_configuration(&yt_dlp_path);
    configuration.ffmpeg_path = ffmpeg_path.display().to_string();
    let error = validate(&configuration).unwrap_err();
    assert_eq!(error.field, ConfigureField::YtDlpPath);
    assert_eq!(error.error, ConfigureError::InvalidToolName(ConfigureField::YtDlpPath));
    fs::remove_file(yt_dlp_path).unwrap();
    fs::remove_file(ffmpeg_path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn validation_rejects_invalid_options_and_concurrency() {
    let directory = std::env::temp_dir().join(format!(
        "yt-dlp-gui-options-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("yt-dlp.exe");
    let ffmpeg_path = directory.join("ffmpeg.exe");
    fs::write(&path, b"test").unwrap();
    fs::write(&ffmpeg_path, b"test").unwrap();
    let mut configuration = valid_configuration(&path);
    configuration.ffmpeg_path = ffmpeg_path.display().to_string();
    configuration.concurrent_downloads = 17;
    assert_eq!(
        validate(&configuration).unwrap_err().field,
        ConfigureField::ConcurrentDownloads
    );
    configuration.concurrent_downloads = 0;
    configuration.language = "fr-FR".to_string();
    assert_eq!(validate(&configuration).unwrap_err().field, ConfigureField::Language);
    fs::remove_file(path).unwrap();
    fs::remove_file(ffmpeg_path).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn path_search_returns_none_for_missing_executable() {
    assert!(find_on_path("yt-dlp-gui-definitely-missing.exe").is_none());
}
