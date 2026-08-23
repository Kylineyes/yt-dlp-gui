mod error;
mod model;
mod parser;
mod process;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub use error::DownloadTaskError;
pub use model::{MediaFormat, MediaMessage, VideoInfo, YtDlpVersion, DEFAULT_METADATA_TIMEOUT};

use model::ClientConfig;

/// yt-dlp 元数据客户端；它只负责检索，不读取存储配置或执行下载。
#[derive(Clone)]
pub struct DownloadTaskClient {
    config: ClientConfig,
}

impl DownloadTaskClient {
    pub fn new(yt_dlp_path: impl Into<PathBuf>, proxy: Option<String>) -> Self {
        Self {
            config: ClientConfig {
                yt_dlp_path: yt_dlp_path.into(),
                proxy: proxy.filter(|value| !value.trim().is_empty()),
                timeout: DEFAULT_METADATA_TIMEOUT,
            },
        }
    }

    pub fn with_timeout(
        yt_dlp_path: impl Into<PathBuf>,
        proxy: Option<String>,
        timeout: Duration,
    ) -> Result<Self, DownloadTaskError> {
        if timeout.is_zero() {
            return Err(DownloadTaskError::InvalidTimeout);
        }
        let mut client = Self::new(yt_dlp_path, proxy);
        client.config.timeout = timeout;
        Ok(client)
    }

    pub fn verify_version(&self) -> Result<YtDlpVersion, DownloadTaskError> {
        process::verify_executable(&self.config)
    }

    /// 在独立线程中检索 URL，并通过回调把状态和元数据转发给调用方。
    pub fn inspect_url<F>(&self, url: impl Into<String>, on_message: F) -> Result<SearchHandle, DownloadTaskError>
    where
        F: Fn(MediaMessage) + Send + 'static,
    {
        let url = url.into();
        if url.trim().is_empty() {
            return Err(DownloadTaskError::EmptyUrl);
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(SharedState::default());
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_shared = Arc::clone(&shared);
        let config = self.config.clone();
        let worker = thread::spawn(move || {
            on_message(MediaMessage::Started);
            let result = process::run_metadata(config, url, Arc::clone(&worker_cancelled))
                .and_then(|output| parser::parse_video_info(&output));
            let message = match &result {
                Ok(video) => {
                    set_result(&worker_shared, video.clone());
                    on_message(MediaMessage::Metadata(video.clone()));
                    MediaMessage::Finished
                }
                Err(DownloadTaskError::Cancelled) => MediaMessage::Cancelled,
                Err(DownloadTaskError::Timeout(_)) => MediaMessage::TimedOut,
                Err(_) => return set_completion(&worker_shared, result),
            };
            set_completion(&worker_shared, result);
            on_message(message);
        });

        Ok(SearchHandle {
            cancelled,
            shared,
            worker: Mutex::new(Some(worker)),
        })
    }
}

/// 任务结果和条件变量必须共享，才能让回调线程写入结果、等待方再安全取出。
#[derive(Default)]
struct SharedState {
    result: Mutex<Option<Result<VideoInfo, DownloadTaskError>>>,
    completion: Condvar,
}

fn set_result(shared: &SharedState, video: VideoInfo) {
    if let Ok(mut result) = shared.result.lock() {
        *result = Some(Ok(video));
    }
}

fn set_completion(shared: &SharedState, result: Result<VideoInfo, DownloadTaskError>) {
    if let Ok(mut stored) = shared.result.lock() {
        *stored = Some(result);
        shared.completion.notify_all();
    }
}

/// 检索任务的控制句柄；丢弃句柄会请求取消，避免遗留 yt-dlp 子进程。
pub struct SearchHandle {
    cancelled: Arc<AtomicBool>,
    shared: Arc<SharedState>,
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
        let result = {
            let mut stored = self.shared.result.lock().map_err(|_| DownloadTaskError::Poisoned)?;
            while stored.is_none() {
                stored = self
                    .shared
                    .completion
                    .wait(stored)
                    .map_err(|_| DownloadTaskError::Poisoned)?;
            }
            stored.take().ok_or(DownloadTaskError::WorkerPanicked)?
        };
        let worker = self
            .worker
            .lock()
            .map_err(|_| DownloadTaskError::Poisoned)?
            .take()
            .ok_or(DownloadTaskError::WorkerPanicked)?;
        worker.join().map_err(|_| DownloadTaskError::WorkerPanicked)?;
        result
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}
