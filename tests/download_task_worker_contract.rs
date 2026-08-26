use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use yt_dlp_gui::download_task::{
    DownloadMessage, DownloadOptions, DownloadRequest, DownloadStage, DownloadTaskClient, DownloadTaskError,
    MediaFormat, VideoInfo,
};
use yt_dlp_gui::storage::{DownloadTaskStatus, EnvironmentConfig, Storage, CONFIG_VERSION};

#[test]
fn download_worker_persists_lifecycle_and_forwards_ffmpeg() {
    let fixture_root = fixture_root();
    std::fs::create_dir_all(&fixture_root).unwrap();
    let fake_yt_dlp = compile_fake_yt_dlp(&fixture_root);
    let target = fixture_root.join("downloads");
    let temporary = fixture_root.join("temporary");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::create_dir_all(&temporary).unwrap();
    let ffmpeg_path = fixture_root.join("ffmpeg bin").join("ffmpeg.exe");
    let database_path = fixture_root.join("download-task.sqlite3");

    Storage::initialize(database_path).unwrap();
    Storage::instance()
        .unwrap()
        .save_configuration(EnvironmentConfig {
            version: CONFIG_VERSION.to_owned(),
            yt_dlp_path: fake_yt_dlp.to_string_lossy().into_owned(),
            ffmpeg_path: ffmpeg_path.to_string_lossy().into_owned(),
            default_download_path: target.to_string_lossy().into_owned(),
            theme: "system".to_owned(),
            language: "zh-CN".to_owned(),
            concurrent_downloads: 1,
            proxy: "http://127.0.0.1:10808".to_owned(),
            search_timeout_sec: 20,
        })
        .unwrap();

    verify_invalid_format_selection(&target, &temporary);
    verify_success(&target, &temporary, &ffmpeg_path);
    verify_process_failure(&fake_yt_dlp, &target, &temporary, &ffmpeg_path);
    verify_timeout(&fake_yt_dlp, &target, &temporary, &ffmpeg_path);
    verify_cancellation(&fake_yt_dlp, &target, &temporary, &ffmpeg_path);
    verify_callback_panic(&fake_yt_dlp, &target, &temporary, &ffmpeg_path);
}

fn verify_invalid_format_selection(target: &Path, temporary: &Path) {
    let client = DownloadTaskClient::from_storage(Duration::from_secs(5)).unwrap();
    let mut invalid = request("invalid", target, temporary);
    invalid.selected_video_format_id = "missing".to_owned();
    assert!(matches!(
        client.download(invalid, |_| {}),
        Err(DownloadTaskError::InvalidDownloadRequest(_))
    ));
}

fn verify_success(target: &Path, temporary: &Path, ffmpeg_path: &Path) {
    let client = DownloadTaskClient::from_storage(Duration::from_secs(5)).unwrap();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client
        .download(request("success", target, temporary), move |message| {
            captured.lock().unwrap().push(message);
        })
        .unwrap();
    let task_id = handle.task_id();
    let result = handle.wait().unwrap();
    assert_eq!(result.task_id, task_id);
    assert_eq!(result.output_path, Some(target.join("fixture.mp4")));

    let stored = Storage::instance()
        .unwrap()
        .get_download_task(task_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.task.status, DownloadTaskStatus::Completed);
    assert_eq!(stored.task.output_path.as_deref(), target.join("fixture.mp4").to_str());
    assert_eq!(stored.task.selected_format.as_deref(), Some("160+251"));
    assert_eq!(stored.streams.len(), 2);
    assert!(stored.streams.iter().all(|stream| stream.progress_percent == Some(100)));
    assert!(stored
        .streams
        .iter()
        .all(|stream| stream.status == DownloadTaskStatus::Completed));
    assert!(stored.streams.iter().all(|stream| stream.started_at.is_some()));
    assert!(stored.streams.iter().all(|stream| stream.finished_at.is_some()));

    let args = std::fs::read_to_string(target.join("fake-args.txt")).unwrap();
    assert_argument_pair(&args, "--ffmpeg-location", &ffmpeg_path.to_string_lossy());
    assert_argument_pair(&args, "--proxy", "http://127.0.0.1:10808");
    assert_argument_pair(&args, "-f", "160+251");
    assert_argument_pair(&args, "--merge-output-format", "mp4");
    assert!(args.lines().any(|argument| argument == "--no-simulate"));

    let messages = messages.lock().unwrap();
    for stage in [
        DownloadStage::Preparing,
        DownloadStage::Downloading,
        DownloadStage::Merging,
        DownloadStage::Completed,
    ] {
        assert!(messages
            .iter()
            .any(|message| matches!(message, DownloadMessage::Progress(progress) if progress.stage == stage)));
    }
    assert!(messages
        .iter()
        .any(|message| matches!(message, DownloadMessage::Merging)));
    assert!(messages
        .iter()
        .any(|message| matches!(message, DownloadMessage::Completed(result) if result.task_id == task_id)));
}

fn verify_process_failure(fake_yt_dlp: &Path, target: &Path, temporary: &Path, ffmpeg_path: &Path) {
    let client = direct_client(fake_yt_dlp, ffmpeg_path, Duration::from_secs(5), target);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client
        .download(request("failure", target, temporary), move |message| {
            captured.lock().unwrap().push(message);
        })
        .unwrap();
    let task_id = handle.task_id();
    assert!(matches!(
        handle.wait(),
        Err(DownloadTaskError::DownloadProcessFailed { status: Some(7), .. })
    ));
    let stored = Storage::instance()
        .unwrap()
        .get_download_task(task_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.task.status, DownloadTaskStatus::Failed);
    assert!(stored
        .streams
        .iter()
        .all(|stream| stream.status == DownloadTaskStatus::Failed));
    assert!(stored
        .task
        .error_message
        .as_ref()
        .is_some_and(|message| message.chars().count() <= 4096));
    let messages = messages.lock().unwrap();
    assert!(
        messages.iter().any(|message| matches!(
            message,
            DownloadMessage::Failed(DownloadTaskError::DownloadProcessFailed { status: Some(7), .. })
        )),
        "未收到匹配的失败消息：{messages:?}"
    );
}

fn verify_timeout(fake_yt_dlp: &Path, target: &Path, temporary: &Path, ffmpeg_path: &Path) {
    let client = direct_client(fake_yt_dlp, ffmpeg_path, Duration::from_millis(100), target);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client
        .download(request("timeout", target, temporary), move |message| {
            captured.lock().unwrap().push(message);
        })
        .unwrap();
    let task_id = handle.task_id();
    assert!(matches!(handle.wait(), Err(DownloadTaskError::Timeout(_))));
    let stored = Storage::instance()
        .unwrap()
        .get_download_task(task_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.task.status, DownloadTaskStatus::Failed);
    assert!(stored
        .streams
        .iter()
        .all(|stream| stream.status == DownloadTaskStatus::Failed));
    assert!(messages
        .lock()
        .unwrap()
        .iter()
        .any(|message| matches!(message, DownloadMessage::Failed(DownloadTaskError::Timeout(_)))));
}

fn verify_cancellation(fake_yt_dlp: &Path, target: &Path, temporary: &Path, ffmpeg_path: &Path) {
    let client = direct_client(fake_yt_dlp, ffmpeg_path, Duration::from_secs(5), target);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client
        .download(request("cancel", target, temporary), move |message| {
            captured.lock().unwrap().push(message);
        })
        .unwrap();
    let task_id = handle.task_id();
    handle.cancel();
    assert!(matches!(handle.wait(), Err(DownloadTaskError::Cancelled)));
    let stored = Storage::instance()
        .unwrap()
        .get_download_task(task_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.task.status, DownloadTaskStatus::Cancelled);
    assert!(stored
        .streams
        .iter()
        .all(|stream| stream.status == DownloadTaskStatus::Cancelled));
    assert!(messages
        .lock()
        .unwrap()
        .iter()
        .any(|message| matches!(message, DownloadMessage::Cancelled)));
}

fn verify_callback_panic(fake_yt_dlp: &Path, target: &Path, temporary: &Path, ffmpeg_path: &Path) {
    let client = direct_client(fake_yt_dlp, ffmpeg_path, Duration::from_secs(5), target);
    let handle = client
        .download(request("success", target, temporary), |message| {
            if matches!(message, DownloadMessage::StreamProgress(_)) {
                panic!("fixture callback panic");
            }
        })
        .unwrap();
    let task_id = handle.task_id();
    assert!(matches!(handle.wait(), Err(DownloadTaskError::WorkerPanicked)));
    let stored = Storage::instance()
        .unwrap()
        .get_download_task(task_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.task.status, DownloadTaskStatus::Failed);
}

fn direct_client(fake_yt_dlp: &Path, ffmpeg_path: &Path, timeout: Duration, target: &Path) -> DownloadTaskClient {
    DownloadTaskClient::new(
        fake_yt_dlp,
        Some(ffmpeg_path.to_owned()),
        Some("http://127.0.0.1:10808".to_owned()),
        timeout,
        target,
    )
}

fn request(mode: &str, target: &Path, temporary: &Path) -> DownloadRequest {
    DownloadRequest {
        source_url: format!("https://download-task.invalid/{mode}"),
        video: VideoInfo {
            id: format!("fixture-{mode}"),
            title: format!("Fixture {mode}"),
            webpage_url: None,
            original_url: None,
            uploader: None,
            channel: None,
            duration_seconds: Some(5.0),
            thumbnail_url: None,
            description: None,
            upload_date: None,
            formats: vec![
                MediaFormat {
                    format_id: Some("160".to_owned()),
                    format_note: Some("144p".to_owned()),
                    extension: Some("mp4".to_owned()),
                    resolution: Some("256x144".to_owned()),
                    width: Some(256),
                    height: Some(144),
                    fps: Some(30.0),
                    filesize: Some(400),
                    filesize_approx: None,
                    bitrate_kbps: None,
                    video_codec: Some("avc1".to_owned()),
                    audio_codec: Some("none".to_owned()),
                    audio_bitrate_kbps: None,
                    video_bitrate_kbps: None,
                    protocol: Some("https".to_owned()),
                    url: None,
                },
                MediaFormat {
                    format_id: Some("251".to_owned()),
                    format_note: Some("audio".to_owned()),
                    extension: Some("webm".to_owned()),
                    resolution: Some("audio only".to_owned()),
                    width: None,
                    height: None,
                    fps: None,
                    filesize: Some(200),
                    filesize_approx: None,
                    bitrate_kbps: None,
                    video_codec: Some("none".to_owned()),
                    audio_codec: Some("opus".to_owned()),
                    audio_bitrate_kbps: None,
                    video_bitrate_kbps: None,
                    protocol: Some("https".to_owned()),
                    url: None,
                },
            ],
        },
        selected_video_format_id: "160".to_owned(),
        selected_audio_format_id: "251".to_owned(),
        output_template: "%(title)s.%(ext)s".to_owned(),
        target_directory: target.to_owned(),
        temporary_directory: temporary.to_owned(),
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions::default(),
    }
}

fn assert_argument_pair(arguments: &str, name: &str, value: &str) {
    let arguments = arguments.lines().collect::<Vec<_>>();
    assert!(arguments.windows(2).any(|pair| pair == [name, value]));
}

fn fixture_root() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("yt-dlp-gui-download-worker-{}-{nonce}", std::process::id()))
}

fn compile_fake_yt_dlp(root: &Path) -> PathBuf {
    let source_path = root.join("fake_yt_dlp.rs");
    let executable_path = root.join("fake-yt-dlp.exe");
    std::fs::write(
        &source_path,
        r#"
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.iter().any(|argument| argument == "--version") {
        println!("2026.08.19");
        return;
    }
    let mode = arguments.last().map(String::as_str).unwrap_or_default();
    if mode.ends_with("failure") {
        eprintln!("{}", "fixture-error-".repeat(500));
        std::process::exit(7);
    }
    if mode.ends_with("timeout") || mode.ends_with("cancel") {
        std::thread::sleep(Duration::from_secs(10));
        return;
    }
    let home = arguments
        .windows(2)
        .find_map(|pair| (pair[0] == "--paths" && pair[1].starts_with("home:")).then(|| &pair[1][5..]))
        .expect("missing home path");
    std::fs::write(PathBuf::from(home).join("fake-args.txt"), arguments.join("\n")).unwrap();
    println!("download:downloading\t160\t100\t400\tNA\t100 B/s\t1\t3\t25%");
    println!("download:finished\t160\t400\t400\tNA\t100 B/s\t4\t0\t100%");
    println!("download:downloading\t251\t50\t200\tNA\t50 B/s\t1\t3\t25%");
    println!("download:finished\t251\t200\t200\tNA\t50 B/s\t4\t0\t100%");
    println!("postprocess:Merger");
    println!("after_move:{}", PathBuf::from(home).join("fixture.mp4").display());
}
"#,
    )
    .unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg("--edition=2021")
        .arg(&source_path)
        .arg("-o")
        .arg(&executable_path)
        .status()
        .unwrap();
    assert!(status.success());
    executable_path
}
