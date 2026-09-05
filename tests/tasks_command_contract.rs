use std::path::{Path, PathBuf};

use yt_dlp_gui::app::tasks::format_yt_dlp_download_command;
use yt_dlp_gui::download_task::{DownloadOptions, DownloadRequest, VideoInfo};

fn request() -> DownloadRequest {
    DownloadRequest {
        source_url: "https://example.invalid/watch?v=1&name=o'connor; Remove-Item *".to_owned(),
        video: VideoInfo {
            id: "video-id".to_owned(),
            title: "Example".to_owned(),
            webpage_url: None,
            original_url: None,
            uploader: None,
            channel: None,
            duration_seconds: None,
            thumbnail_url: None,
            description: None,
            upload_date: None,
            formats: Vec::new(),
        },
        selected_video_format_id: "137".to_owned(),
        selected_audio_format_id: "140".to_owned(),
        output_template: "%(title).80B [%(id)s].%(ext)s".to_owned(),
        target_directory: PathBuf::from(r"C:\Downloads"),
        temporary_directory: PathBuf::from(r"C:\Downloads\.yt-dlp-gui-temp"),
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions {
            rate_limit: Some("2M".to_owned()),
            retries: Some(3),
            fragment_retries: Some(4),
            file_access_retries: Some(5),
            concurrent_fragments: Some(6),
        },
    }
}

#[test]
fn copied_command_recreates_the_saved_download_parameters() {
    let command = format_yt_dlp_download_command(
        &request(),
        Some(Path::new(r"C:\Program Files\ffmpeg\bin")),
        Some("socks5://proxy.example:1080"),
    );

    assert_eq!(
        command,
        concat!(
            "yt-dlp '-f' '137+140' '--merge-output-format' 'mp4' '--no-overwrites' '-P' 'home:C:\\Downloads' ",
            "'-P' 'temp:C:\\Downloads\\.yt-dlp-gui-temp' '-o' '%(title).80B [%(id)s].%(ext)s' '--proxy' ",
            "'socks5://proxy.example:1080' '--ffmpeg-location' 'C:\\Program Files\\ffmpeg\\bin' '--limit-rate' ",
            "'2M' '--retries' '3' '--fragment-retries' '4' '--file-access-retries' '5' '--concurrent-fragments' ",
            "'6' 'https://example.invalid/watch?v=1&name=o''connor; Remove-Item *'"
        )
    );
}

#[test]
fn copied_command_omits_blank_optional_configuration() {
    let mut request = request();
    request.options = DownloadOptions::default();
    let command = format_yt_dlp_download_command(&request, None, Some("  "));

    assert!(!command.contains("'--proxy'"));
    assert!(!command.contains("'--ffmpeg-location'"));
    assert!(!command.contains("'--limit-rate'"));
    assert!(command.ends_with("'https://example.invalid/watch?v=1&name=o''connor; Remove-Item *'"));
}
