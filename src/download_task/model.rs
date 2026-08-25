use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_METADATA_TIMEOUT: Duration = Duration::from_secs(20);

/// yt-dlp 的版本信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtDlpVersion {
    /// yt-dlp 输出的原始版本字符串，例如 `2026.08.19`。
    pub value: String,
}

/// yt-dlp 返回的单个视频元数据。
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    /// 视频在站点上的唯一标识。
    pub id: String,
    /// 视频标题。
    pub title: String,
    /// 视频规范页面地址。
    pub webpage_url: Option<String>,
    /// 发起检索时传入的原始地址。
    pub original_url: Option<String>,
    /// 发布者或上传者名称。
    pub uploader: Option<String>,
    /// 所属频道名称。
    pub channel: Option<String>,
    /// 视频时长，单位为秒。
    pub duration_seconds: Option<f64>,
    /// 视频缩略图地址。
    pub thumbnail_url: Option<String>,
    /// 视频描述文本。
    pub description: Option<String>,
    /// 发布日期，通常为 `YYYYMMDD` 格式。
    pub upload_date: Option<String>,
    /// 视频可用的媒体格式列表。
    pub formats: Vec<MediaFormat>,
}

/// 视频或音频媒体格式的可选属性。
#[derive(Debug, Clone, PartialEq)]
pub struct MediaFormat {
    /// yt-dlp 分配的格式标识。
    pub format_id: Option<String>,
    /// 格式的补充说明，例如音频或视频类型。
    pub format_note: Option<String>,
    /// 媒体文件扩展名。
    pub extension: Option<String>,
    /// 面向用户展示的分辨率文本。
    pub resolution: Option<String>,
    /// 视频宽度，单位为像素。
    pub width: Option<u64>,
    /// 视频高度，单位为像素。
    pub height: Option<u64>,
    /// 视频帧率，单位为 FPS。
    pub fps: Option<f64>,
    /// 精确文件大小，单位为字节。
    pub filesize: Option<u64>,
    /// 估算文件大小，单位为字节。
    pub filesize_approx: Option<u64>,
    /// 综合码率，单位为 Kbps。
    pub bitrate_kbps: Option<f64>,
    /// 视频编码器名称。
    pub video_codec: Option<String>,
    /// 音频编码器名称。
    pub audio_codec: Option<String>,
    /// 音频码率，单位为 Kbps。
    pub audio_bitrate_kbps: Option<f64>,
    /// 视频码率，单位为 Kbps。
    pub video_bitrate_kbps: Option<f64>,
    /// yt-dlp 使用的传输协议。
    pub protocol: Option<String>,
    /// 当前格式的媒体地址；部分站点可能不返回或返回临时地址。
    pub url: Option<String>,
}

/// 检索任务向调用方发送的状态消息。
#[derive(Debug, Clone, PartialEq)]
pub enum MediaMessage {
    Started,
    Metadata(VideoInfo),
    Finished,
    Cancelled,
    TimedOut,
}

pub const DEFAULT_PROGRESS_DELTA: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadMediaType {
    Video,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStreamStatus {
    Downloading,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStage {
    Preparing,
    Downloading,
    Merging,
    Completed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRequest {
    pub source_url: String,
    pub video: VideoInfo,
    pub selected_video_format_id: String,
    pub selected_audio_format_id: String,
    pub output_template: String,
    pub target_directory: PathBuf,
    pub temporary_directory: PathBuf,
    pub merge_output_format: String,
    pub options: DownloadOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadOptions {
    pub rate_limit: Option<String>,
    pub retries: Option<u32>,
    pub fragment_retries: Option<u32>,
    pub file_access_retries: Option<u32>,
    pub concurrent_fragments: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamProgress {
    pub stream_key: String,
    pub format_id: Option<String>,
    pub media_type: DownloadMediaType,
    pub status: DownloadStreamStatus,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub total_bytes_estimate: Option<i64>,
    pub speed_bytes_per_second: Option<i64>,
    pub elapsed_seconds: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub percent: Option<u8>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadProgress {
    pub task_id: i64,
    pub stage: DownloadStage,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub total_bytes_estimate: Option<i64>,
    pub speed_bytes_per_second: Option<i64>,
    pub elapsed_seconds: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub percent: Option<u8>,
    pub total_is_estimate: bool,
    pub active_stream: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadResult {
    pub task_id: i64,
    pub output_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum DownloadMessage {
    Started,
    StreamProgress(StreamProgress),
    Progress(DownloadProgress),
    Merging,
    Completed(DownloadResult),
    Cancelled,
    Failed(crate::download_task::DownloadTaskError),
}

impl DownloadRequest {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.source_url.trim().is_empty()
            || self.selected_video_format_id.trim().is_empty()
            || self.selected_audio_format_id.trim().is_empty()
            || self.output_template.trim().is_empty()
            || self.merge_output_format.trim().is_empty()
        {
            return Err("下载请求包含空字段".to_owned());
        }
        if !matches!(self.merge_output_format.as_str(), "mp4" | "mkv") {
            return Err("合并容器只能是 mp4 或 mkv".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientConfig {
    /// yt-dlp 可执行文件路径。
    pub(crate) yt_dlp_path: PathBuf,
    /// 可选的 FFmpeg 可执行文件或 bin 目录路径。
    pub(crate) ffmpeg_path: Option<PathBuf>,
    /// 可选的网络代理地址。
    pub(crate) proxy: Option<String>,
    /// 元数据检索和下载任务共用的超时时间；`None` 表示不设超时。
    pub(crate) timeout: Option<Duration>,
    /// 默认下载目录快照；下载请求仍需显式给出最终目录。
    pub(crate) storage_path: PathBuf,
}
