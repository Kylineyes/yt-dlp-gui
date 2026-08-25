mod error;
mod model;
mod parser;
mod persistence;
mod process;

use std::panic::{catch_unwind, AssertUnwindSafe};
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

use model::ClientConfig;
use persistence::PersistedDownload;

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

#[derive(Clone)]
pub struct DownloadTaskClient {
    config: ClientConfig,
}

impl DownloadTaskClient {
    pub fn new(
        yt_dlp_path: impl Into<PathBuf>,
        ffmpeg_path: Option<PathBuf>,
        proxy: Option<String>,
        timeout: Duration,
        storage_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config: ClientConfig {
                yt_dlp_path: yt_dlp_path.into(),
                ffmpeg_path,
                proxy: proxy.filter(|value| !value.trim().is_empty()),
                timeout: (!timeout.is_zero()).then_some(timeout),
                storage_path: storage_path.into(),
            },
        }
    }

    pub fn from_storage(timeout: Duration) -> Result<Self, DownloadTaskError> {
        let configuration = crate::storage::Storage::instance()
            .map_err(|error| DownloadTaskError::Storage(error.to_string()))?
            .configuration()
            .map_err(|error| DownloadTaskError::Storage(error.to_string()))?
            .ok_or_else(|| DownloadTaskError::Storage("配置数据库中不存在环境配置记录".to_owned()))?;
        let ffmpeg_path = (!configuration.ffmpeg_path.trim().is_empty()).then(|| configuration.ffmpeg_path.into());
        Ok(Self::new(
            configuration.yt_dlp_path,
            ffmpeg_path,
            Some(configuration.proxy),
            timeout,
            configuration.default_download_path,
        ))
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
            let panic_shared = Arc::clone(&worker_shared);
            let result = catch_unwind(AssertUnwindSafe(|| {
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
            }));
            if result.is_err() {
                set_search_completion(&panic_shared, Err(DownloadTaskError::WorkerPanicked));
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
        let version = process::verify_executable(&self.config)?;
        let now = unix_timestamp();
        let persisted = PersistedDownload::create(&request, version.value, now)?;
        let task_id = persisted.task_id();
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(DownloadSharedState {
            result: Mutex::new(None),
            latest_progress: Mutex::new(Some(empty_progress(task_id, DownloadStage::Preparing, now))),
            output_path: Mutex::new(None),
            completion: Condvar::new(),
        });
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_shared = Arc::clone(&shared);
        let config = self.config.clone();
        let worker = thread::spawn(move || {
            let panic_shared = Arc::clone(&worker_shared);
            let result = catch_unwind(AssertUnwindSafe(|| {
                run_download_worker(config, request, persisted, worker_cancelled, worker_shared, on_message);
            }));
            if result.is_err() {
                set_download_completion(&panic_shared, Err(DownloadTaskError::WorkerPanicked));
            }
        });
        Ok(DownloadHandle {
            task_id,
            cancelled,
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
}

fn run_download_worker<F>(
    config: ClientConfig,
    request: DownloadRequest,
    mut persisted: PersistedDownload,
    cancelled: Arc<AtomicBool>,
    shared: Arc<DownloadSharedState>,
    on_message: F,
) where
    F: Fn(DownloadMessage),
{
    let task_id = persisted.task_id();
    on_message(DownloadMessage::Started);
    if let Some(preparing) = shared.latest_progress.lock().ok().and_then(|progress| progress.clone()) {
        on_message(DownloadMessage::Progress(preparing));
    }
    let now = unix_timestamp();
    if let Err(error) = persisted.mark_downloading(now) {
        finish_failed_download(&shared, &persisted, error, &on_message);
        return;
    }
    let initial = empty_progress(task_id, DownloadStage::Downloading, now);
    set_latest_progress(&shared, initial.clone());
    on_message(DownloadMessage::Progress(initial));

    let mut streams: Vec<StreamProgress> = Vec::new();
    let result = process::run_download(config, request, Arc::clone(&cancelled), |output| {
        match output {
            parser::DownloadOutput::Progress(mut progress) => {
                let now = unix_timestamp();
                update_stream_timestamps(&mut streams, &mut progress, now);
                if let Some(existing) = streams.iter_mut().find(|item| item.stream_key == progress.stream_key) {
                    *existing = progress.clone();
                } else {
                    streams.push(progress.clone());
                }
                on_message(DownloadMessage::StreamProgress(progress.clone()));
                let task_progress = parser::aggregate_progress(task_id, &streams, now);
                persisted.write_progress(
                    &task_progress,
                    &progress,
                    progress.status == DownloadStreamStatus::Finished,
                );
                set_latest_progress(&shared, task_progress.clone());
                on_message(DownloadMessage::Progress(task_progress));
            }
            parser::DownloadOutput::Merging => {
                let now = unix_timestamp();
                if persisted.mark_merging(now)? {
                    let progress = stage_progress(&shared, task_id, DownloadStage::Merging, now);
                    set_latest_progress(&shared, progress.clone());
                    on_message(DownloadMessage::Progress(progress));
                    on_message(DownloadMessage::Merging);
                }
            }
            parser::DownloadOutput::OutputPath(path) => set_output_path(&shared, path),
        }
        Ok(())
    });

    match result {
        Ok(output_path) => {
            let output_path = output_path
                .or_else(|| latest_output_path(&shared))
                .ok_or(DownloadTaskError::OutputPathMissing);
            match output_path {
                Ok(output_path) => {
                    let now = unix_timestamp();
                    let mut progress = stage_progress(&shared, task_id, DownloadStage::Completed, now);
                    progress.percent = Some(100);
                    progress.active_stream = None;
                    persisted.write_final_progress(&progress, &streams);
                    if let Err(error) = persisted.complete(output_path.clone(), now) {
                        finish_failed_download(&shared, &persisted, error, &on_message);
                        return;
                    }
                    set_latest_progress(&shared, progress.clone());
                    on_message(DownloadMessage::Progress(progress));
                    let result = DownloadResult {
                        task_id,
                        output_path: Some(output_path.into()),
                    };
                    set_download_completion(&shared, Ok(result.clone()));
                    on_message(DownloadMessage::Completed(result));
                }
                Err(error) => finish_failed_download(&shared, &persisted, error, &on_message),
            }
        }
        Err(DownloadTaskError::Cancelled) => {
            let now = unix_timestamp();
            match persisted.cancel(now) {
                Ok(()) => {
                    set_download_completion(&shared, Err(DownloadTaskError::Cancelled));
                    on_message(DownloadMessage::Cancelled);
                }
                Err(error) => finish_failed_download(&shared, &persisted, error, &on_message),
            }
        }
        Err(error) => finish_failed_download(&shared, &persisted, error, &on_message),
    }
}

fn finish_failed_download<F>(
    shared: &DownloadSharedState,
    persisted: &PersistedDownload,
    error: DownloadTaskError,
    on_message: &F,
) where
    F: Fn(DownloadMessage),
{
    let now = unix_timestamp();
    let final_error = match persisted.fail(&error, now) {
        Ok(()) => error,
        Err(storage_error) => DownloadTaskError::Storage(format!("{error}；{storage_error}")),
    };
    set_download_completion(shared, Err(final_error.clone()));
    on_message(DownloadMessage::Failed(final_error));
}

fn update_stream_timestamps(streams: &mut [StreamProgress], progress: &mut StreamProgress, now: i64) {
    progress.started_at = streams
        .iter()
        .find(|item| item.stream_key == progress.stream_key)
        .and_then(|item| item.started_at)
        .or(Some(now));
    if progress.status == DownloadStreamStatus::Finished {
        progress.finished_at = Some(now);
    }
}

fn empty_progress(task_id: i64, stage: DownloadStage, updated_at: i64) -> DownloadProgress {
    DownloadProgress {
        task_id,
        stage,
        downloaded_bytes: 0,
        total_bytes: None,
        total_bytes_estimate: None,
        speed_bytes_per_second: None,
        elapsed_seconds: None,
        eta_seconds: None,
        percent: None,
        total_is_estimate: false,
        active_stream: None,
        updated_at,
    }
}

fn stage_progress(
    shared: &DownloadSharedState,
    task_id: i64,
    stage: DownloadStage,
    updated_at: i64,
) -> DownloadProgress {
    let mut progress = shared
        .latest_progress
        .lock()
        .ok()
        .and_then(|progress| progress.clone())
        .unwrap_or_else(|| empty_progress(task_id, stage, updated_at));
    progress.stage = stage;
    progress.updated_at = updated_at;
    progress.active_stream = None;
    progress
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
        let result = wait_for_result(&self.shared.result, &self.shared.completion);
        join_worker(&self.worker)?;
        result
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

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
    task_id: i64,
    cancelled: Arc<AtomicBool>,
    shared: Arc<DownloadSharedState>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl DownloadHandle {
    pub fn task_id(&self) -> i64 {
        self.task_id
    }

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
        let result = wait_for_result(&self.shared.result, &self.shared.completion);
        join_worker(&self.worker)?;
        result
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
