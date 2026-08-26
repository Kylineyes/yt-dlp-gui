use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use yt_dlp_gui::download_task::{DownloadOptions, DownloadRequest, DownloadTaskClient, MediaFormat};
use yt_dlp_gui::storage::{EnvironmentConfig, Storage, CONFIG_VERSION};

#[test]
#[ignore = "需要显式提供真实 yt-dlp、FFmpeg、URL 和代理环境变量"]
fn downloads_lowest_video_quality_with_configured_ffmpeg() {
    let yt_dlp_path = required_path("YTDLP_GUI_TEST_YT_DLP");
    let ffmpeg_path = required_path("YTDLP_GUI_TEST_FFMPEG");
    let source_url = required_string("YTDLP_GUI_TEST_URL");
    let proxy = std::env::var("YTDLP_GUI_TEST_PROXY").unwrap_or_default();
    let root = acceptance_root();
    let target = root.join("downloads");
    let temporary = root.join("temporary");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(&temporary).unwrap();

    Storage::initialize(root.join("acceptance.sqlite3")).unwrap();
    Storage::instance()
        .unwrap()
        .save_configuration(EnvironmentConfig {
            version: CONFIG_VERSION.to_owned(),
            yt_dlp_path: yt_dlp_path.to_string_lossy().into_owned(),
            ffmpeg_path: ffmpeg_path.to_string_lossy().into_owned(),
            default_download_path: target.to_string_lossy().into_owned(),
            theme: "system".to_owned(),
            language: "zh-CN".to_owned(),
            concurrent_downloads: 1,
            proxy,
            search_timeout_sec: 20,
        })
        .unwrap();

    let client = DownloadTaskClient::from_storage(Duration::from_secs(600)).unwrap();
    let video = client.inspect_url(source_url.clone(), |_| {}).unwrap().wait().unwrap();
    let selected_video = lowest_video_stream(&video.formats);
    let selected_audio = compatible_audio_stream(&video.formats);
    let selected_video_format_id = selected_video.format_id.clone().unwrap();
    let selected_audio_format_id = selected_audio.format_id.clone().unwrap();
    let selected_video_height = selected_video.height.unwrap();
    let selected_video_note = selected_video.format_note.clone().unwrap_or_default();
    println!(
        "真实下载选择：视频格式 {}（{}，实际高度 {}），音频格式 {}",
        selected_video_format_id, selected_video_note, selected_video_height, selected_audio_format_id
    );
    assert!(selected_video_note.contains("144p"));

    let request = DownloadRequest {
        source_url,
        video,
        selected_video_format_id,
        selected_audio_format_id,
        output_template: "%(title).80B [%(id)s].%(ext)s".to_owned(),
        target_directory: target.clone(),
        temporary_directory: temporary,
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions::default(),
    };
    let result = client
        .download(request, |message| println!("{message:?}"))
        .unwrap()
        .wait()
        .unwrap();
    let output_path = result.output_path.expect("真实下载必须返回最终路径");
    assert!(output_path.starts_with(&target));
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.is_file());
    assert!(metadata.len() > 0);
    println!("真实下载完成：{}（{} 字节）", output_path.display(), metadata.len());
}

fn lowest_video_stream(formats: &[MediaFormat]) -> &MediaFormat {
    formats
        .iter()
        .filter(|format| {
            format.format_id.is_some()
                && format.height.is_some()
                && format
                    .video_codec
                    .as_deref()
                    .is_some_and(|codec| codec != "none" && !codec.is_empty())
                && format
                    .audio_codec
                    .as_deref()
                    .is_none_or(|codec| codec == "none" || codec.is_empty())
        })
        .min_by_key(|format| {
            (
                format.height.unwrap(),
                format.bitrate_kbps.map(|value| value as u64).unwrap_or(u64::MAX),
            )
        })
        .expect("没有可用的视频流")
}

fn compatible_audio_stream(formats: &[MediaFormat]) -> &MediaFormat {
    formats
        .iter()
        .filter(|format| {
            format.format_id.is_some()
                && format.extension.as_deref() == Some("m4a")
                && format
                    .video_codec
                    .as_deref()
                    .is_none_or(|codec| codec == "none" || codec.is_empty())
                && format
                    .audio_codec
                    .as_deref()
                    .is_some_and(|codec| codec != "none" && !codec.is_empty())
        })
        .min_by_key(|format| format.audio_bitrate_kbps.map(|value| value as u64).unwrap_or(u64::MAX))
        .expect("没有可用的 M4A 音频流")
}

fn required_path(name: &str) -> PathBuf {
    PathBuf::from(required_string(name))
}

fn required_string(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("缺少环境变量：{name}"))
}

fn acceptance_root() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("yt-dlp-gui-real-download-{}-{nonce}", std::process::id()))
}
