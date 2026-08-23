use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use yt_dlp_gui::download_task::{DownloadTaskClient, DownloadTaskError, MediaMessage, DEFAULT_METADATA_TIMEOUT};

const REAL_YT_DLP: &str = r"C:\Users\derek\scoop\shims\yt-dlp.exe";
const TEST_URL: &str = "https://www.youtube.com/watch?v=MAQdHTeiSoM&list=TLPQMTgwNzIwMjL3gKTk28fb8A&index=19";

#[test]
fn exposes_twenty_second_default_timeout() {
    assert_eq!(DEFAULT_METADATA_TIMEOUT, Duration::from_secs(20));
}

#[test]
fn rejects_zero_timeout() {
    assert!(matches!(
        DownloadTaskClient::with_timeout("yt-dlp.exe", None, Duration::ZERO),
        Err(DownloadTaskError::InvalidTimeout)
    ));
}

#[test]
fn rejects_empty_url_before_starting_worker() {
    let client = DownloadTaskClient::new(PathBuf::from("yt-dlp.exe"), Some("  ".to_owned()));
    let result = client.inspect_url("  ", |_| {});
    assert!(matches!(result, Err(DownloadTaskError::EmptyUrl)));
}

#[test]
fn verifies_missing_executable_without_spawning() {
    let client = DownloadTaskClient::new("path-that-does-not-exist.exe", None);
    assert!(matches!(
        client.verify_version(),
        Err(DownloadTaskError::ExecutableNotFound(_))
    ));
}

#[test]
fn cancelled_task_reports_cancelled_terminal_message() {
    let client = DownloadTaskClient::new(REAL_YT_DLP, None);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client.inspect_url("https://example.com/video", move |message| {
        captured.lock().unwrap().push(message);
    });
    let handle = handle.unwrap();
    handle.cancel();
    let result = handle.wait();
    assert!(matches!(result, Err(DownloadTaskError::Cancelled)));
    assert!(messages
        .lock()
        .unwrap()
        .iter()
        .any(|message| matches!(message, MediaMessage::Cancelled)));
}

#[test]
#[ignore = "requires network access and the locally installed yt-dlp executable"]
fn inspects_the_requested_video_without_downloading() {
    let client = DownloadTaskClient::new(REAL_YT_DLP, None);
    let version = client.verify_version().unwrap();
    assert!(!version.value.is_empty());

    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client
        .inspect_url(TEST_URL, move |message| {
            captured.lock().unwrap().push(message);
        })
        .unwrap();
    let result = handle.wait().unwrap();
    assert_eq!(result.id, "MAQdHTeiSoM");
    assert!(!result.title.is_empty());
    assert_eq!(result.formats.len(), 19);
}
