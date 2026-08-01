use crate::download::ytdlp::{self, DownloadRequest};
use crate::storage::Storage;
use std::path::PathBuf;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub mod settings_validation;
pub mod state;
use state::{AppSettings, DownloadRecord, MediaStream, NewDownload, SearchResult};

pub enum UiCommand {
    LoadSettings,
    SaveSettings(AppSettings),
    Search { url: String },
    CreateDownload(NewDownload),
    StartDownload { id: i64 },
    RefreshDownloads,
    PauseDownload { id: i64 },
    Shutdown,
}

#[derive(Debug, Clone, Copy)]
pub enum NoticeKind {
    PauseUnsupported,
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    General,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Ready,
    SettingsLoaded(AppSettings),
    SettingsSaved(AppSettings),
    SearchCompleted(SearchResult),
    DownloadsLoaded(Vec<DownloadRecord>),
    DownloadStarted { id: i64 },
    LogLine { id: i64, line: String },
    DownloadFinished { id: i64 },
    DownloadFailed { id: i64, message: String },
    Notice { kind: NoticeKind, argument: String },
    Error { kind: ErrorKind, detail: String },
}

pub fn spawn_worker(
    mut commands: UnboundedReceiver<UiCommand>,
    events: UnboundedSender<WorkerEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create the Tokio runtime");
        runtime.block_on(async move {
            let storage = match Storage::open_default() {
                Ok(storage) => storage,
                Err(error) => {
                    let _ = events.send(WorkerEvent::Error {
                        kind: ErrorKind::General,
                        detail: error.to_string(),
                    });
                    return;
                }
            };
            let _ = events.send(WorkerEvent::Ready);
            send_initial_state(&storage, &events);

            while let Some(command) = commands.recv().await {
                match command {
                    UiCommand::LoadSettings => match storage.load_settings() {
                        Ok(settings) => {
                            let _ = events.send(WorkerEvent::SettingsLoaded(settings));
                        }
                        Err(error) => send_error(&events, error),
                    },
                    UiCommand::SaveSettings(settings) => match storage.save_settings(&settings) {
                        Ok(()) => {
                            let _ = events.send(WorkerEvent::SettingsSaved(settings));
                        }
                        Err(error) => send_error(&events, error),
                    },
                    UiCommand::Search { url } => {
                        let _ = events.send(WorkerEvent::SearchCompleted(mock_search(&url)));
                    }
                    UiCommand::CreateDownload(download) => {
                        let result = storage
                            .create_download(&download)
                            .and_then(|_| storage.list_downloads());
                        match result {
                            Ok(downloads) => {
                                let _ = events.send(WorkerEvent::DownloadsLoaded(downloads));
                            }
                            Err(error) => send_error(&events, error),
                        }
                    }
                    UiCommand::StartDownload { id } => {
                        run_download(id, &storage, &events).await;
                        if let Ok(downloads) = storage.list_downloads() {
                            let _ = events.send(WorkerEvent::DownloadsLoaded(downloads));
                        }
                    }
                    UiCommand::RefreshDownloads => match storage.list_downloads() {
                        Ok(downloads) => {
                            let _ = events.send(WorkerEvent::DownloadsLoaded(downloads));
                        }
                        Err(error) => send_error(&events, error),
                    },
                    UiCommand::PauseDownload { id } => {
                        let _ = events.send(WorkerEvent::Notice {
                            kind: NoticeKind::PauseUnsupported,
                            argument: id.to_string(),
                        });
                    }
                    UiCommand::Shutdown => break,
                }
            }
        });
    })
}

fn send_initial_state(storage: &Storage, events: &UnboundedSender<WorkerEvent>) {
    if let Ok(settings) = storage.load_settings() {
        let _ = events.send(WorkerEvent::SettingsLoaded(settings));
    }
    if let Ok(downloads) = storage.list_downloads() {
        let _ = events.send(WorkerEvent::DownloadsLoaded(downloads));
    }
}

fn send_error(events: &UnboundedSender<WorkerEvent>, error: impl std::fmt::Display) {
    let _ = events.send(WorkerEvent::Error {
        kind: ErrorKind::General,
        detail: error.to_string(),
    });
}

fn mock_search(url: &str) -> SearchResult {
    let suffix = url
        .split('/')
        .next_back()
        .filter(|value| !value.is_empty())
        .unwrap_or("resource");
    SearchResult {
        resource_name: suffix.to_string(),
        streams: vec![
            MediaStream {
                id: "best-mp4".into(),
                label: "Audio/video · 1080p · MP4 · H.264 + AAC".into(),
                format_selector: "bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]".into(),
                video_format: "1080p H.264 / MP4".into(),
                audio_format: "AAC / M4A".into(),
                estimated_size: "Size will be determined by yt-dlp".into(),
            },
            MediaStream {
                id: "best-720p".into(),
                label: "Audio/video · 720p · MP4".into(),
                format_selector: "bestvideo[height<=720]+bestaudio/best[height<=720]".into(),
                video_format: "Up to 720p / MP4".into(),
                audio_format: "Best available audio".into(),
                estimated_size: "Size will be determined by yt-dlp".into(),
            },
            MediaStream {
                id: "audio".into(),
                label: "Audio only · Best quality".into(),
                format_selector: "bestaudio/best".into(),
                video_format: "No video".into(),
                audio_format: "Best available audio".into(),
                estimated_size: "Size will be determined by yt-dlp".into(),
            },
        ],
    }
}

async fn run_download(id: i64, storage: &Storage, events: &UnboundedSender<WorkerEvent>) {
    let record = match storage.get_download(id) {
        Ok(record) => record,
        Err(error) => {
            send_error(events, error);
            return;
        }
    };
    let settings = match storage.load_settings() {
        Ok(settings) => settings,
        Err(error) => {
            send_error(events, error);
            return;
        }
    };
    if let Err(error) = storage.mark_started(id) {
        send_error(events, error);
        return;
    }
    let _ = events.send(WorkerEvent::DownloadStarted { id });
    let mut sequence = 0_i64;
    let result = ytdlp::download(
        DownloadRequest {
            url: record.url,
            output_directory: PathBuf::from(record.output_directory),
            yt_dlp_path: optional_path(&settings.yt_dlp_path),
            ffmpeg_path: optional_path(&settings.ffmpeg_path),
            proxy: optional_string(&settings.proxy),
            format_selector: record.format_selector,
        },
        |line| {
            sequence += 1;
            let _ = storage.append_log(id, sequence, &line);
            let _ = events.send(WorkerEvent::LogLine { id, line });
        },
    )
    .await;
    match result {
        Ok(()) => {
            if let Err(error) = storage.mark_completed(id) {
                send_error(events, error);
            } else {
                let _ = events.send(WorkerEvent::DownloadFinished { id });
            }
        }
        Err(error) => {
            let message = error.to_string();
            let _ = storage.mark_failed(id, &message);
            let _ = events.send(WorkerEvent::DownloadFailed { id, message });
        }
    }
}

fn optional_path(value: &str) -> Option<PathBuf> {
    (!value.trim().is_empty()).then(|| PathBuf::from(value))
}

fn optional_string(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}
