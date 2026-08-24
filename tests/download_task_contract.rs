use std::sync::{Arc, Mutex};
use std::time::Duration;

use yt_dlp_gui::download_task::{
    aggregate_download_progress, parse_download_progress_line, DownloadMediaType, DownloadStreamStatus,
    DownloadTaskClient, DownloadTaskError, MediaMessage, DEFAULT_METADATA_TIMEOUT,
};

const FAKE_URL: &str = "https://download-task.invalid/watch?v=metadata-fixture";
#[test]
fn parses_structured_stream_progress_fixture() {
    let progress = parse_download_progress_line(
        "download:downloading\t137\t1048576\t4194304\tNA\t1 MiB/s\t4\t3\t25%",
        "137",
        "251",
    )
    .unwrap()
    .unwrap();
    assert_eq!(progress.media_type, DownloadMediaType::Video);
    assert_eq!(progress.status, DownloadStreamStatus::Downloading);
    assert_eq!(progress.downloaded_bytes, 1_048_576);
    assert_eq!(progress.total_bytes, Some(4_194_304));
    assert_eq!(progress.percent, Some(25));
}

#[test]
fn aggregates_exact_and_estimated_stream_sizes() {
    let video = parse_download_progress_line(
        "download:downloading\t137\t100\t400\tNA\t100 B/s\t4\t3\t25%",
        "137",
        "251",
    )
    .unwrap()
    .unwrap();
    let audio = parse_download_progress_line("download:finished\t251\t50\tNA\t200\t50 B/s\t2\tNA\t100%", "137", "251")
        .unwrap()
        .unwrap();
    let progress = aggregate_download_progress(7, &[video, audio], 10);
    assert_eq!(progress.downloaded_bytes, 150);
    assert_eq!(progress.total_bytes, None);
    assert_eq!(progress.total_bytes_estimate, Some(600));
    assert!(progress.total_is_estimate);
    assert_eq!(progress.percent, Some(25));
}

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
