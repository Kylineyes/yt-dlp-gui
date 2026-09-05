use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use yt_dlp_gui::download_task::{
    DownloadMessage, DownloadOptions, DownloadOutcome, DownloadRequest, DownloadTaskClient,
};
use yt_dlp_gui::storage::{DownloadTaskStatus, Storage};

#[test]
#[ignore = "需要 YTDLP_GUI_TEST_YT_DLP 和 YTDLP_GUI_TEST_FFMPEG，使用本地 HTTP 媒体验证真实续传"]
fn real_ytdlp_resumes_local_media_without_recreating_records() {
    let executable = PathBuf::from(std::env::var_os("YTDLP_GUI_TEST_YT_DLP").expect("缺少真实 yt-dlp 路径"));
    let ffmpeg = PathBuf::from(std::env::var_os("YTDLP_GUI_TEST_FFMPEG").expect("缺少真实 FFmpeg 路径"));
    let root = std::env::temp_dir().join(format!(
        "real-pause-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    create_media(&ffmpeg, &root);
    let server = MediaServer::start(root.clone());
    Storage::initialize(root.join("tasks.sqlite")).unwrap();
    let client = DownloadTaskClient::new(
        executable,
        Some(ffmpeg),
        Some(String::new()),
        Duration::from_secs(90),
        root.join("output"),
    );
    let url = format!("http://{}/manifest.mpd", server.address);
    let video = client.inspect_url(url.clone(), |_| {}).unwrap().wait().unwrap();
    let video_id = video
        .formats
        .iter()
        .find(|format| format.video_codec.as_deref().is_some_and(|codec| codec != "none"))
        .unwrap()
        .format_id
        .clone()
        .unwrap();
    let audio_id = video
        .formats
        .iter()
        .find(|format| format.audio_codec.as_deref().is_some_and(|codec| codec != "none"))
        .unwrap()
        .format_id
        .clone()
        .unwrap();
    let (tx, rx) = mpsc::channel();
    let notified = AtomicBool::new(false);
    let request = DownloadRequest {
        source_url: url,
        video,
        selected_video_format_id: video_id,
        selected_audio_format_id: audio_id,
        output_template: "real-resume.%(ext)s".to_owned(),
        target_directory: root.join("output"),
        temporary_directory: root.join("temporary"),
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions {
            rate_limit: Some("128K".to_owned()),
            concurrent_fragments: Some(1),
            ..DownloadOptions::default()
        },
    };
    let handle = client.download(request, move |message| {
        if matches!(message, DownloadMessage::Progress(ref progress) if progress.downloaded_bytes >= 32768 && progress.percent != Some(100)) && !notified.swap(true, Ordering::SeqCst) {
            tx.send(()).unwrap();
        }
    }).unwrap();
    let id = handle.task_id();
    rx.recv_timeout(Duration::from_secs(40)).unwrap();
    handle.pause().unwrap();
    assert_eq!(handle.wait_outcome().unwrap(), DownloadOutcome::Paused { task_id: id });
    let storage = Storage::instance().unwrap();
    let paused = storage.get_download_task(id).unwrap().unwrap();
    assert_eq!(paused.task.status, DownloadTaskStatus::Paused);
    assert!(paused.task.downloaded_bytes >= 32768);
    let files = nonempty_files(&root.join("temporary"));
    assert!(
        files.iter().any(|path| path.to_string_lossy().contains(".part")),
        "暂停时必须保留真实续传文件：{files:?}"
    );
    let ranges_before = server.ranges.load(Ordering::SeqCst);
    let result = client
        .resume_download(id, |message| println!("{message:?}"))
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(result.task_id, id);
    let finished = storage.get_download_task(id).unwrap().unwrap();
    assert_eq!(finished.task.status, DownloadTaskStatus::Completed);
    assert!(finished
        .streams
        .iter()
        .all(|stream| stream.status == DownloadTaskStatus::Completed));
    assert_eq!(finished.task.started_at, paused.task.started_at);
    assert_eq!(
        finished.streams.iter().map(|stream| stream.id).collect::<Vec<_>>(),
        paused.streams.iter().map(|stream| stream.id).collect::<Vec<_>>()
    );
    assert!(
        server.ranges.load(Ordering::SeqCst) > ranges_before,
        "真实续传必须发出非零偏移的 Range 请求"
    );
    let output = result.output_path.unwrap();
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    let ffmpeg = std::env::var_os("YTDLP_GUI_TEST_FFMPEG").unwrap();
    let decoded = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        decoded.status.success(),
        "合并文件必须能完整解码：{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    println!(
        "真实暂停/继续通过，输出：{}，暂停累计字节：{}",
        output.display(),
        paused.task.downloaded_bytes
    );
}

fn create_media(ffmpeg: &Path, root: &Path) {
    for (source, codec, output) in [
        (
            "testsrc2=size=320x180:rate=24",
            vec!["-c:v", "libx264", "-preset", "ultrafast", "-crf", "24", "-an"],
            "video.mp4",
        ),
        (
            "sine=frequency=440:sample_rate=44100",
            vec!["-c:a", "aac", "-b:a", "96k", "-vn"],
            "audio.m4a",
        ),
    ] {
        let result = Command::new(ffmpeg)
            .args(["-v", "error", "-f", "lavfi", "-i", source, "-t", "12"])
            .args(codec)
            .args(["-movflags", "+faststart"])
            .arg(root.join(output))
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    }
    std::fs::write(root.join("manifest.mpd"), r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT12S" minBufferTime="PT1S" profiles="urn:mpeg:dash:profile:isoff-on-demand:2011">
<Period duration="PT12S">
<AdaptationSet mimeType="video/mp4" contentType="video"><Representation id="v" bandwidth="600000" codecs="avc1.42c01e" width="320" height="180" frameRate="24"><BaseURL>video.mp4</BaseURL></Representation></AdaptationSet>
<AdaptationSet mimeType="audio/mp4" contentType="audio"><Representation id="a" bandwidth="96000" codecs="mp4a.40.2" audioSamplingRate="44100"><BaseURL>audio.m4a</BaseURL></Representation></AdaptationSet>
</Period></MPD>"#).unwrap();
}

fn nonempty_files(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.metadata().unwrap().len() > 0)
        .collect()
}

struct MediaServer {
    address: std::net::SocketAddr,
    ranges: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl MediaServer {
    fn start(root: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let ranges = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_ranges = Arc::clone(&ranges);
        let worker_stopped = Arc::clone(&stopped);
        let worker = thread::spawn(move || {
            let mut connections = Vec::new();
            while !worker_stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let root = root.clone();
                        let ranges = Arc::clone(&worker_ranges);
                        connections.push(thread::spawn(move || {
                            let _ = serve(stream, &root, &ranges);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("本地测试服务器失败：{error}"),
                }
            }
            for connection in connections {
                connection.join().unwrap();
            }
        });
        Self {
            address,
            ranges,
            stopped,
            worker: Some(worker),
        }
    }
}

impl Drop for MediaServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn serve(mut stream: TcpStream, root: &Path, ranges: &AtomicUsize) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let request = line.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
    if request.len() < 2 {
        return Ok(());
    }
    let mut range = None;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
            let (start, end) = value.trim().split_once('-').unwrap();
            range = Some((start.parse::<usize>().unwrap(), end.parse::<usize>().ok()));
        }
    }
    let name = request[1].trim_start_matches('/').split('?').next().unwrap();
    if !matches!(name, "manifest.mpd" | "video.mp4" | "audio.m4a") {
        return stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
    let content = std::fs::read(root.join(name))?;
    let (start, end) = range
        .map(|(start, end)| (start, end.unwrap_or(content.len() - 1).min(content.len() - 1)))
        .unwrap_or((0, content.len() - 1));
    if start >= content.len() {
        return stream
            .write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
    if start > 0 {
        ranges.fetch_add(1, Ordering::SeqCst);
    }
    let status = if range.is_some() {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let content_type = if name.ends_with("mpd") {
        "application/dash+xml"
    } else {
        "video/mp4"
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n", end - start + 1)?;
    if range.is_some() {
        write!(stream, "Content-Range: bytes {start}-{end}/{}\r\n", content.len())?;
    }
    stream.write_all(b"\r\n")?;
    if request[0] != "HEAD" {
        stream.write_all(&content[start..=end])?;
    }
    Ok(())
}
