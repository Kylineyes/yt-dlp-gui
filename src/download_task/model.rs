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
    /// worker 线程已经开始处理检索。
    Started,
    /// 已完成 JSON 解析并获得视频元数据。
    Metadata(VideoInfo),
    /// 检索成功结束。
    Finished,
    /// 检索被调用方主动取消。
    Cancelled,
    /// 检索超过配置的超时时间。
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientConfig {
    /// yt-dlp 可执行文件路径。
    pub(crate) yt_dlp_path: PathBuf,
    /// 可选的网络代理地址。
    pub(crate) proxy: Option<String>,
    /// 当前客户端使用的元数据检索超时时间。
    pub(crate) timeout: Duration,
}
