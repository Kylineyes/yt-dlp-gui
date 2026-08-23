use std::sync::{Arc, Mutex};
use std::time::Duration;

use yt_dlp_gui::download_task::{DownloadTaskClient, DownloadTaskError, MediaMessage, DEFAULT_METADATA_TIMEOUT};

const FAKE_URL: &str = "https://download-task.invalid/watch?v=metadata-fixture";

#[test]
fn exposes_twenty_second_default_timeout() {
    assert_eq!(DEFAULT_METADATA_TIMEOUT, Duration::from_secs(20));
}

#[test]
fn zero_timeout_disables_deadline() {
    let client = DownloadTaskClient::new("yt-dlp.exe", None, Duration::ZERO, "fixture-output");
    assert!(client.inspect_url(FAKE_URL, |_| {}).is_ok());
}

#[test]
fn empty_url_is_reported_by_yt_dlp() {
    let client = DownloadTaskClient::new("path-that-does-not-exist.exe", None, Duration::ZERO, "fixture-output");
    let handle = client.inspect_url("", |_| {}).unwrap();
    assert!(matches!(
        handle.wait(),
        Err(DownloadTaskError::ProcessFailed { .. }) | Err(DownloadTaskError::ExecutableNotFound(_))
    ));
}

#[test]
fn verifies_missing_executable_without_spawning() {
    let client = DownloadTaskClient::new("path-that-does-not-exist.exe", None, Duration::ZERO, "fixture-output");
    assert!(matches!(
        client.verify_version(),
        Err(DownloadTaskError::ExecutableNotFound(_))
    ));
}

#[test]
fn cancelled_task_reports_cancelled_terminal_message() {
    let client = DownloadTaskClient::new("path-that-does-not-exist.exe", None, Duration::ZERO, "fixture-output");
    let messages = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&messages);
    let handle = client.inspect_url(FAKE_URL, move |message| {
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
