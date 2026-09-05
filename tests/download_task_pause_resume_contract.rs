use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use yt_dlp_gui::download_task::{
    DownloadMessage, DownloadOptions, DownloadOutcome, DownloadRequest, DownloadStage, DownloadStreamStatus,
    DownloadTaskClient, DownloadTaskError, MediaFormat, VideoInfo,
};
use yt_dlp_gui::storage::{DownloadTaskDraft, DownloadTaskStatus, Storage};

#[test]
fn pause_resume_preserves_snapshot_identity_and_quiesces_old_session() {
    let root = std::env::temp_dir().join(format!(
        "download-pause-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let executable = compile_fixture(&root);
    Storage::initialize(root.join("tasks.sqlite")).unwrap();
    verify_pause_resume(&root, &executable);
    verify_resume_validation(&root, &executable);
    verify_merging_rejects_pause(&root, &executable);
    verify_spawn_failure(&root, &executable);
    verify_initial_callback_panic(&root, &executable);
}

fn client(executable: &Path, root: &Path) -> DownloadTaskClient {
    DownloadTaskClient::new(executable, None, None, Duration::from_secs(15), root)
}

fn verify_pause_resume(root: &Path, executable: &Path) {
    let target = root.join("pause");
    let request = request(&target, "pause");
    let original_options = request.options.clone();
    let client = client(executable, root);
    let count = Arc::new(AtomicUsize::new(0));
    let events = Arc::clone(&count);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let handle = client.download(request, move |message| {
        events.fetch_add(1, Ordering::SeqCst);
        if matches!(message, DownloadMessage::StreamProgress(ref progress) if progress.stream_key == "a" && progress.status == DownloadStreamStatus::Downloading) {
            ready_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        }
    }).unwrap();
    let id = handle.task_id();
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    handle.request_pause().unwrap();
    assert!(!handle.is_cancelled());
    assert!(client.resume_download(id, |_| {}).is_err(), "旧回调尚未结束时不得继续");
    release_tx.send(()).unwrap();
    assert_eq!(handle.wait_outcome().unwrap(), DownloadOutcome::Paused { task_id: id });
    let count_at_pause = count.load(Ordering::SeqCst);
    let storage = Storage::instance().unwrap();
    let paused = storage.get_download_task(id).unwrap().unwrap();
    assert_eq!(paused.task.status, DownloadTaskStatus::Paused);
    assert!(paused.task.finished_at.is_none());
    assert_eq!(paused.task.downloaded_bytes, 140);
    assert!(paused.task.speed_bytes_per_second.is_none());
    assert!(paused.task.elapsed_seconds.is_none());
    assert!(paused.task.eta_seconds.is_none());
    let video = paused.streams.iter().find(|stream| stream.stream_key == "v").unwrap();
    assert_eq!(video.status, DownloadTaskStatus::Completed);
    let audio = paused.streams.iter().find(|stream| stream.stream_key == "a").unwrap();
    assert_eq!(audio.status, DownloadTaskStatus::Paused);
    assert_eq!(audio.downloaded_bytes, 40);
    assert!(audio.finished_at.is_none());
    assert_eq!(
        std::fs::read(target.join("tmp").join("audio.part")).unwrap(),
        vec![7; 40]
    );
    let snapshot = storage.load_download_execution_snapshot(id).unwrap().unwrap();
    assert_eq!(snapshot.output_template, "frozen.%(ext)s");
    assert_eq!(snapshot.options.rate_limit, original_options.rate_limit);
    assert_eq!(
        snapshot.options.concurrent_fragments,
        original_options.concurrent_fragments
    );
    assert_eq!(snapshot.temporary_directory, target.join("tmp").to_str().unwrap());

    std::fs::write(target.join("finish"), b"continue").unwrap();
    let newer_client = DownloadTaskClient::new(
        executable,
        Some(root.join("new-ffmpeg")),
        Some("http://127.0.0.1:9000".to_owned()),
        Duration::from_secs(15),
        root.join("changed-default"),
    );
    let first_progress = Arc::new(Mutex::new(None));
    let initial = Arc::clone(&first_progress);
    let resumed = newer_client
        .resume_download(id, move |message| {
            if let DownloadMessage::Progress(progress) = message {
                assert!(progress.downloaded_bytes >= 140, "恢复进度不得归零");
                let mut initial = initial.lock().unwrap();
                if initial.is_none() {
                    *initial = Some(progress);
                }
            }
        })
        .unwrap();
    let completed = resumed.wait().unwrap();
    assert_eq!(completed.task_id, id);
    assert_eq!(
        first_progress.lock().unwrap().as_ref().unwrap().stage,
        DownloadStage::Preparing
    );
    let finished = storage.get_download_task(id).unwrap().unwrap();
    assert_eq!(finished.task.status, DownloadTaskStatus::Completed);
    assert_eq!(finished.task.started_at, paused.task.started_at);
    assert_eq!(
        finished.streams.iter().map(|stream| stream.id).collect::<Vec<_>>(),
        paused.streams.iter().map(|stream| stream.id).collect::<Vec<_>>()
    );
    assert_eq!(
        finished.streams.iter().find(|stream| stream.stream_key == "v").unwrap(),
        video
    );
    assert!(finished
        .streams
        .iter()
        .all(|stream| stream.status == DownloadTaskStatus::Completed));
    assert_eq!(storage.load_download_execution_snapshot(id).unwrap().unwrap(), snapshot);
    assert_eq!(count.load(Ordering::SeqCst), count_at_pause, "旧会话不得再发送消息");
    assert!(newer_client.resume_download(id, |_| {}).is_err());
    let args = std::fs::read_to_string(target.join("args.txt")).unwrap();
    for (name, value) in [
        ("--output", "frozen.%(ext)s"),
        ("--limit-rate", "128K"),
        ("--retries", "3"),
        ("--concurrent-fragments", "2"),
        ("--proxy", "http://127.0.0.1:9000"),
        ("-f", "v+a"),
    ] {
        assert!(
            args.lines()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| pair == [name, value]),
            "缺少参数 {name}"
        );
    }
    assert!(args.lines().any(|argument| argument == "--ignore-config"));
    assert!(args.lines().any(|argument| argument == "--continue"));
}

fn verify_resume_validation(root: &Path, executable: &Path) {
    let storage = Storage::instance().unwrap();
    let legacy = storage
        .create_download_task(
            DownloadTaskDraft {
                source_url: "https://fixture.invalid/legacy".to_owned(),
                video_id: None,
                title: None,
                thumbnail_url: None,
                duration_seconds: None,
                target_path: root.to_string_lossy().into_owned(),
                output_path: None,
                selected_format: None,
                created_at: 1,
                yt_dlp_version: None,
            },
            vec![],
        )
        .unwrap();
    storage
        .update_download_status(legacy.id, DownloadTaskStatus::Preparing, 2)
        .unwrap();
    storage.pause_download_task(legacy.id, 3).unwrap();
    let client = client(executable, root);
    assert!(client.resume_download(legacy.id, |_| {}).is_err());
    assert!(
        client.resume_download(legacy.id, |_| {}).is_err(),
        "验证失败须释放会话登记"
    );
    assert_eq!(
        storage.get_download_task(legacy.id).unwrap().unwrap().task.status,
        DownloadTaskStatus::Paused
    );

    let target = root.join("directory-validation");
    let (tx, rx) = mpsc::channel();
    let handle = client
        .download(request(&target, "pause"), move |message| {
            if matches!(message, DownloadMessage::StreamProgress(ref progress) if progress.stream_key == "a") {
                tx.send(()).unwrap();
            }
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let id = handle.task_id();
    client.pause_download(id).unwrap();
    assert_eq!(handle.wait_outcome().unwrap(), DownloadOutcome::Paused { task_id: id });
    std::fs::rename(target.join("tmp"), target.join("moved-tmp")).unwrap();
    assert!(client.resume_download(id, |_| {}).is_err());
    assert_eq!(
        storage.get_download_task(id).unwrap().unwrap().task.status,
        DownloadTaskStatus::Paused
    );
    std::fs::rename(target.join("moved-tmp"), target.join("tmp")).unwrap();
    let missing_tool = root.join("missing-tool.exe");
    assert!(
        DownloadTaskClient::new(&missing_tool, None, None, Duration::from_secs(15), root)
            .resume_download(id, |_| {})
            .is_err()
    );
    assert_eq!(
        storage.get_download_task(id).unwrap().unwrap().task.status,
        DownloadTaskStatus::Paused
    );
}

fn verify_merging_rejects_pause(root: &Path, executable: &Path) {
    let client = client(executable, root);
    let target = root.join("merging");
    let (tx, rx) = mpsc::channel();
    let handle = client
        .download(request(&target, "merging"), move |message| {
            if matches!(message, DownloadMessage::Merging) {
                tx.send(()).unwrap();
            }
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(handle.request_pause().is_err());
    std::fs::write(target.join("finish"), b"finish").unwrap();
    handle.wait().unwrap();
}

fn verify_spawn_failure(root: &Path, executable: &Path) {
    let copy = root.join("removed-after-version.exe");
    std::fs::copy(executable, &copy).unwrap();
    let client = client(&copy, root);
    let target = root.join("spawn-failure");
    let (tx, rx) = mpsc::channel();
    let handle = client
        .download(request(&target, "pause"), move |message| {
            if matches!(message, DownloadMessage::StreamProgress(ref progress) if progress.stream_key == "a") {
                tx.send(()).unwrap();
            }
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let id = handle.task_id();
    handle.pause().unwrap();
    handle.wait_outcome().unwrap();
    let resumed = client
        .resume_download(id, move |message| {
            if matches!(message, DownloadMessage::Started) {
                std::fs::remove_file(&copy).unwrap();
            }
        })
        .unwrap();
    assert!(matches!(
        resumed.wait_outcome(),
        Err(DownloadTaskError::ExecutableNotFound(_))
    ));
    let stored = Storage::instance().unwrap().get_download_task(id).unwrap().unwrap();
    assert_eq!(stored.task.status, DownloadTaskStatus::Failed);
    assert!(stored.streams.iter().all(|stream| stream.status.is_terminal()));
}

fn verify_initial_callback_panic(root: &Path, executable: &Path) {
    let client = client(executable, root);
    let handle = client
        .download(request(&root.join("panic"), "pause"), |message| {
            if matches!(message, DownloadMessage::Started) {
                panic!("准备阶段回调异常");
            }
        })
        .unwrap();
    let id = handle.task_id();
    assert!(matches!(handle.wait_outcome(), Err(DownloadTaskError::WorkerPanicked)));
    assert_eq!(
        Storage::instance()
            .unwrap()
            .get_download_task(id)
            .unwrap()
            .unwrap()
            .task
            .status,
        DownloadTaskStatus::Failed
    );
}

fn request(target: &Path, mode: &str) -> DownloadRequest {
    DownloadRequest {
        source_url: format!("https://fixture.invalid/{mode}"),
        video: VideoInfo {
            id: "fixture".to_owned(),
            title: "Frozen title".to_owned(),
            webpage_url: None,
            original_url: None,
            uploader: None,
            channel: None,
            duration_seconds: Some(2.0),
            thumbnail_url: None,
            description: None,
            upload_date: None,
            formats: vec![format("v", true), format("a", false)],
        },
        selected_video_format_id: "v".to_owned(),
        selected_audio_format_id: "a".to_owned(),
        output_template: "frozen.%(ext)s".to_owned(),
        target_directory: target.to_owned(),
        temporary_directory: target.join("tmp"),
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions {
            rate_limit: Some("128K".to_owned()),
            retries: Some(3),
            fragment_retries: Some(4),
            file_access_retries: Some(5),
            concurrent_fragments: Some(2),
        },
    }
}

fn format(id: &str, video: bool) -> MediaFormat {
    MediaFormat {
        format_id: Some(id.to_owned()),
        format_note: None,
        extension: Some("mp4".to_owned()),
        resolution: None,
        width: None,
        height: None,
        fps: None,
        filesize: None,
        filesize_approx: None,
        bitrate_kbps: None,
        video_codec: Some(if video { "h264" } else { "none" }.to_owned()),
        audio_codec: Some(if video { "none" } else { "aac" }.to_owned()),
        audio_bitrate_kbps: None,
        video_bitrate_kbps: None,
        protocol: None,
        url: None,
    }
}

fn compile_fixture(root: &Path) -> PathBuf {
    let source = root.join("fake.rs");
    let executable = root.join("fake.exe");
    std::fs::write(&source, r#"
use std::path::PathBuf;
use std::time::Duration;
use std::io::Write;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version") { println!("2026.08.19"); return; }
    let path = |prefix: &str| PathBuf::from(args.windows(2).find_map(|pair| (pair[0] == "--paths").then(|| pair[1].strip_prefix(prefix)).flatten()).unwrap());
    let home = path("home:");
    let temp = path("temp:");
    std::fs::write(home.join("args.txt"), args.join("\n")).unwrap();
    let merging = args.last().unwrap().ends_with("merging");
    let resumed = temp.join("audio.part").exists();
    println!("download:finished\tv\t100\t100\tNA\t100 B/s\t1\t0\t100%");
    if !merging && !resumed {
        std::fs::write(temp.join("audio.part"), vec![7; 40]).unwrap();
        println!("download:downloading\ta\t40\t100\tNA\t40 B/s\t1\t2\t40%");
        std::io::stdout().flush().unwrap();
        loop { std::thread::sleep(Duration::from_millis(10)); }
    }
    if resumed { assert_eq!(std::fs::read(temp.join("audio.part")).unwrap(), vec![7; 40]); }
    println!("download:finished\ta\t100\t100\tNA\t100 B/s\t1\t0\t100%");
    println!("postprocess:Merger");
    std::io::stdout().flush().unwrap();
    while !home.join("finish").exists() { std::thread::sleep(Duration::from_millis(10)); }
    std::fs::write(home.join("frozen.mp4"), b"fixture").unwrap();
    println!("after_move:{}", home.join("frozen.mp4").display());
}
"#).unwrap();
    let status = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--edition=2021")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap();
    assert!(status.success());
    executable
}
