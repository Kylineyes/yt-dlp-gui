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
    StorageInitializationFailed {
        detail: String,
    },
    #[allow(dead_code)]
    PromptRequested {
        title: String,
        message: String,
    },
    SettingsLoaded(AppSettings),
    SettingsSaved(AppSettings),
    SearchCompleted(SearchResult),
    SearchFailed {
        detail: String,
    },
    DownloadCreated {
        id: i64,
    },
    DownloadCreateFailed {
        detail: String,
    },
    DownloadsLoaded(Vec<DownloadRecord>),
    DownloadStarted {
        id: i64,
    },
    LogLine {
        id: i64,
        line: String,
    },
    DownloadFinished {
        id: i64,
    },
    DownloadFailed {
        id: i64,
        message: String,
    },
    Notice {
        kind: NoticeKind,
        argument: String,
    },
    Error {
        kind: ErrorKind,
        detail: String,
    },
}

#[allow(dead_code)]
pub fn request_prompt(
    events: &UnboundedSender<WorkerEvent>,
    title: impl Into<String>,
    message: impl Into<String>,
) {
    let _ = events.send(WorkerEvent::PromptRequested {
        title: title.into(),
        message: message.into(),
    });
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
            let Some(storage) = initialize_storage(&events, Storage::open_default) else {
                return;
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
                        let settings = match storage.load_settings() {
                            Ok(settings) => settings,
                            Err(error) => {
                                let _ = events.send(WorkerEvent::SearchFailed {
                                    detail: error.to_string(),
                                });
                                continue;
                            }
                        };
                        let yt_dlp_path = match storage.require_yt_dlp_path() {
                            Ok(path) => path,
                            Err(error) => {
                                let _ = events.send(WorkerEvent::SearchFailed {
                                    detail: error.to_string(),
                                });
                                continue;
                            }
                        };
                        let result = ytdlp::inspect(ytdlp::InspectRequest {
                            url,
                            yt_dlp_path,
                            proxy: optional_string(&settings.proxy),
                        })
                        .await;
                        match result {
                            Ok(resources) => {
                                if resources.len() != 1 {
                                    let detail = if resources.is_empty() {
                                        "No media metadata was returned by yt-dlp".into()
                                    } else {
                                        "Playlist inspection is not supported; enter a single media URL".into()
                                    };
                                    let _ = events.send(WorkerEvent::SearchFailed { detail });
                                    continue;
                                }
                                let resource = resources.into_iter().next().unwrap_or_else(|| unreachable!());
                                if resource.formats.is_empty() {
                                    let _ = events.send(WorkerEvent::SearchFailed {
                                        detail: "yt-dlp returned no downloadable formats".into(),
                                    });
                                    continue;
                                }
                                let streams = resource
                                    .formats
                                    .into_iter()
                                    .map(|format| {
                                        let id = format.id;
                                        MediaStream {
                                            id: id.clone(),
                                            label: format.label,
                                            format_selector: id,
                                            video_format: format.video_format,
                                            audio_format: format.audio_format,
                                            estimated_size: format.estimated_size,
                                        }
                                    })
                                    .collect();
                                let _ = events.send(WorkerEvent::SearchCompleted(SearchResult {
                                    resource_name: resource.resource_name,
                                    streams,
                                }));
                            }
                            Err(error) => {
                                let _ = events.send(WorkerEvent::SearchFailed {
                                    detail: error.to_string(),
                                });
                            }
                        }
                    }
                    UiCommand::CreateDownload(download) => {
                        if let Err(error) = storage.require_yt_dlp_path() {
                            let _ = events.send(WorkerEvent::DownloadCreateFailed {
                                detail: error.to_string(),
                            });
                            continue;
                        }
                        match storage.create_download(&download) {
                            Ok(id) => match storage.list_downloads() {
                                Ok(downloads) => {
                                    let _ = events.send(WorkerEvent::DownloadsLoaded(downloads));
                                    let _ = events.send(WorkerEvent::DownloadCreated { id });
                                }
                                Err(error) => {
                                    let _ = events.send(WorkerEvent::DownloadCreated { id });
                                    send_error(&events, error);
                                }
                            }
                            Err(error) => {
                                let _ = events.send(WorkerEvent::DownloadCreateFailed {
                                    detail: error.to_string(),
                                });
                            }
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

fn initialize_storage(
    events: &UnboundedSender<WorkerEvent>,
    open_storage: impl FnOnce() -> Result<Storage, crate::error::AppError>,
) -> Option<Storage> {
    match open_storage() {
        Ok(storage) => Some(storage),
        Err(error) => {
            let _ = events.send(WorkerEvent::StorageInitializationFailed {
                detail: error.to_string(),
            });
            None
        }
    }
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
    let yt_dlp_path = match storage.require_yt_dlp_path() {
        Ok(path) => path,
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
            yt_dlp_path,
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
    (!value.is_empty()).then(|| PathBuf::from(value))
}

fn optional_string(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AppError, StorageStage};
    use std::path::PathBuf;
    use tokio::sync::mpsc::error::TryRecvError;

    #[test]
    fn prompt_request_preserves_title_and_message() {
        let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        request_prompt(&events, "Download failed", "The output path is unavailable");

        match receiver.try_recv().expect("Expected a prompt request") {
            WorkerEvent::PromptRequested { title, message } => {
                assert_eq!(title, "Download failed");
                assert_eq!(message, "The output path is unavailable");
            }
            other => panic!("Expected a prompt request event, got {other:?}"),
        }
    }

    #[test]
    fn storage_initialization_failure_sends_dedicated_event() {
        let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let database_path = PathBuf::from("C:/restricted/application.sqlite3");
        let expected_detail = AppError::StorageIo {
            stage: StorageStage::OpenDatabase,
            path: Some(database_path.clone()),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        }
        .to_string();

        let storage = initialize_storage(&events, || {
            Err(AppError::StorageIo {
                stage: StorageStage::OpenDatabase,
                path: Some(database_path),
                source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
            })
        });

        assert!(storage.is_none());
        let event = receiver
            .try_recv()
            .expect("Expected a storage initialization failure event");
        match event {
            WorkerEvent::StorageInitializationFailed { detail } => {
                assert_eq!(detail, expected_detail);
            }
            other => {
                panic!("Expected a dedicated storage initialization failure event, got {other:?}")
            }
        }
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }
}
