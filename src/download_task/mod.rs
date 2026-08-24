mod error;
mod model;
mod parser;
mod process;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use error::DownloadTaskError;
pub use model::{
    DownloadMediaType, DownloadMessage, DownloadOptions, DownloadProgress, DownloadRequest, DownloadResult,
    DownloadStage, DownloadStreamStatus, MediaFormat, MediaMessage, StreamProgress, VideoInfo, YtDlpVersion,
    DEFAULT_METADATA_TIMEOUT, DEFAULT_PROGRESS_DELTA,
};

pub fn parse_download_progress_line(
    line: &str,
    video_format_id: &str,
    audio_format_id: &str,
) -> Result<Option<StreamProgress>, DownloadTaskError> {
    match parser::parse_download_line(line, video_format_id, audio_format_id)? {
        Some(parser::DownloadOutput::Progress(progress)) => Ok(Some(progress)),
        Some(parser::DownloadOutput::Merging | parser::DownloadOutput::OutputPath(_)) | None => Ok(None),
    }
}

pub fn aggregate_download_progress(task_id: i64, streams: &[StreamProgress], updated_at: i64) -> DownloadProgress {
    parser::aggregate_progress(task_id, streams, updated_at)
}

use model::ClientConfig;

#[derive(Clone)]
pub struct DownloadTaskClient {
    config: ClientConfig,
}

impl DownloadTaskClient {
    pub fn new(
        yt_dlp_path: impl Into<PathBuf>,
        proxy: Option<String>,
        timeout: Duration,
        storage_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config: ClientConfig {
                yt_dlp_path: yt_dlp_path.into(),
                proxy: proxy.filter(|value| !value.trim().is_empty()),
                timeout: (!timeout.is_zero()).then_some(timeout),
                storage_path: storage_path.into(),
            },
        }
    }

    pub fn verify_version(&self) -> Result<YtDlpVersion, DownloadTaskError> {
        process::verify_executable(&self.config)
    }

    pub fn storage_path(&self) -> &std::path::Path {
        &self.config.storage_path
    }

    pub fn inspect_url<F>(&self, url: impl Into<String>, on_message: F) -> Result<SearchHandle, DownloadTaskError>
    where
        F: Fn(MediaMessage) + Send + 'static,
    {
        let url = url.into();
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(SearchSharedState::default());
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_shared = Arc::clone(&shared);
        let config = self.config.clone();
        let worker = thread::spawn(move || {
            on_message(MediaMessage::Started);
            let result = process::run_metadata(config, url, Arc::clone(&worker_cancelled))
                .and_then(|output| parser::parse_video_info(&output));
            match result {
                Ok(video) => {
                    set_search_completion(&worker_shared, Ok(video.clone()));
                    on_message(MediaMessage::Metadata(video));
                    on_message(MediaMessage::Finished);
                }
                Err(DownloadTaskError::Cancelled) => {
                    set_search_completion(&worker_shared, Err(DownloadTaskError::Cancelled));
                    on_message(MediaMessage::Cancelled);
                }
                Err(error @ DownloadTaskError::Timeout(_)) => {
                    set_search_completion(&worker_shared, Err(error));
                    on_message(MediaMessage::TimedOut);
                }
                Err(error) => set_search_completion(&worker_shared, Err(error)),
            }
        });
        Ok(SearchHandle {
            cancelled,
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn download<F>(&self, request: DownloadRequest, on_message: F) -> Result<DownloadHandle, DownloadTaskError>
    where
        F: Fn(DownloadMessage) + Send + 'static,
    {
        request.validate().map_err(DownloadTaskError::InvalidDownloadRequest)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(DownloadSharedState::default());
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_shared = Arc::clone(&shared);
        let config = self.config.clone();
        let worker = thread::spawn(move || {
            on_message(DownloadMessage::Started);
            let mut streams: Vec<StreamProgress> = Vec::new();
            let result = process::run_download(config, request.clone(), worker_cancelled, |output| match output {
                parser::DownloadOutput::Progress(progress) => {
                    if let Some(existing) = streams.iter_mut().find(|item| item.stream_key == progress.stream_key) {
                        *existing = progress.clone();
                    } else {
                        streams.push(progress.clone());
                    }
                    on_message(DownloadMessage::StreamProgress(progress));
                    let progress = parser::aggregate_progress(request.task_id, &streams, unix_timestamp());
                    set_latest_progress(&worker_shared, progress.clone());
                    on_message(DownloadMessage::Progress(progress));
                }
                parser::DownloadOutput::Merging => on_message(DownloadMessage::Merging),
                parser::DownloadOutput::OutputPath(path) => set_output_path(&worker_shared, path),
            });
            match result {
                Ok(output_path) => {
                    let output_path = output_path.or_else(|| latest_output_path(&worker_shared));
                    let result = DownloadResult {
                        task_id: request.task_id,
                        output_path: output_path.map(Into::into),
                    };
                    set_download_completion(&worker_shared, Ok(result.clone()));
                    on_message(DownloadMessage::Completed(result));
                }
                Err(DownloadTaskError::Cancelled) => {
                    set_download_completion(&worker_shared, Err(DownloadTaskError::Cancelled));
                    on_message(DownloadMessage::Cancelled);
                }
                Err(error) => {
                    let message = error.to_string();
                    let error_for_callback = format!("下载任务失败：{message}");
                    set_download_completion(&worker_shared, Err(error));
                    on_message(DownloadMessage::Failed(DownloadTaskError::DownloadProcessFailed {
                        status: None,
                        stderr: error_for_callback,
                    }));
                }
            }
        });
        Ok(DownloadHandle {
            cancelled,
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
}

#[derive(Default)]
struct SearchSharedState {
    result: Mutex<Option<Result<VideoInfo, DownloadTaskError>>>,
    completion: Condvar,
}

fn set_search_completion(shared: &SearchSharedState, result: Result<VideoInfo, DownloadTaskError>) {
    if let Ok(mut stored) = shared.result.lock() {
        *stored = Some(result);
        shared.completion.notify_all();
    }
}

pub struct SearchHandle {
    cancelled: Arc<AtomicBool>,
    shared: Arc<SearchSharedState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl SearchHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn latest_result(&self) -> Option<VideoInfo> {
        self.shared
            .result
            .lock()
            .ok()
            .and_then(|result| result.as_ref()?.as_ref().ok().cloned())
    }

    pub fn wait(self) -> Result<VideoInfo, DownloadTaskError> {
        let result = wait_for_result(&self.shared.result, &self.shared.completion)?;
        join_worker(&self.worker)?;
        Ok(result)
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Default)]
struct DownloadSharedState {
    result: Mutex<Option<Result<DownloadResult, DownloadTaskError>>>,
    latest_progress: Mutex<Option<DownloadProgress>>,
    output_path: Mutex<Option<String>>,
    completion: Condvar,
}

fn set_download_completion(shared: &DownloadSharedState, result: Result<DownloadResult, DownloadTaskError>) {
    if let Ok(mut stored) = shared.result.lock() {
        *stored = Some(result);
        shared.completion.notify_all();
    }
}

fn set_latest_progress(shared: &DownloadSharedState, progress: DownloadProgress) {
    if let Ok(mut stored) = shared.latest_progress.lock() {
        *stored = Some(progress);
    }
}

fn set_output_path(shared: &DownloadSharedState, path: String) {
    if let Ok(mut stored) = shared.output_path.lock() {
        *stored = Some(path);
    }
}

fn latest_output_path(shared: &DownloadSharedState) -> Option<String> {
    shared.output_path.lock().ok().and_then(|path| path.clone())
}

pub struct DownloadHandle {
    cancelled: Arc<AtomicBool>,
    shared: Arc<DownloadSharedState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl DownloadHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn latest_progress(&self) -> Option<DownloadProgress> {
        self.shared
            .latest_progress
            .lock()
            .ok()
            .and_then(|progress| progress.clone())
    }

    pub fn wait(self) -> Result<DownloadResult, DownloadTaskError> {
        let result = wait_for_result(&self.shared.result, &self.shared.completion)?;
        join_worker(&self.worker)?;
        Ok(result)
    }
}

impl Drop for DownloadHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn wait_for_result<T>(
    result: &Mutex<Option<Result<T, DownloadTaskError>>>,
    completion: &Condvar,
) -> Result<T, DownloadTaskError> {
    let mut stored = result.lock().map_err(|_| DownloadTaskError::Poisoned)?;
    while stored.is_none() {
        stored = completion.wait(stored).map_err(|_| DownloadTaskError::Poisoned)?;
    }
    stored.take().ok_or(DownloadTaskError::WorkerPanicked)?
}

fn join_worker(worker: &Mutex<Option<JoinHandle<()>>>) -> Result<(), DownloadTaskError> {
    let worker = worker
        .lock()
        .map_err(|_| DownloadTaskError::Poisoned)?
        .take()
        .ok_or(DownloadTaskError::WorkerPanicked)?;
    worker.join().map_err(|_| DownloadTaskError::WorkerPanicked)
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
