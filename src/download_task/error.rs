use std::path::PathBuf;
use std::time::Duration;

/// 检索错误保留类别和进程诊断，不暴露完整命令行以避免泄露代理认证信息。
#[derive(Debug)]
pub enum DownloadTaskError {
    ExecutableNotFound(PathBuf),
    Spawn(std::io::Error),
    Io(std::io::Error),
    VersionCommandFailed { status: Option<i32>, stderr: String },
    VersionOutputEmpty,
    ProcessFailed { status: Option<i32>, stderr: String },
    InvalidJson(String),
    MissingField(&'static str),
    InvalidField { field: &'static str, message: String },
    Timeout(Duration),
    Cancelled,
    Poisoned,
    WorkerPanicked,
    InvalidDownloadRequest(String),
    ProgressParse(String),
    DownloadProcessFailed { status: Option<i32>, stderr: String },
}
impl std::fmt::Display for DownloadTaskError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExecutableNotFound(path) => write!(formatter, "找不到 yt-dlp 文件：{}。", path.display()),
            Self::Spawn(error) => write!(formatter, "无法启动 yt-dlp：{error}"),
            Self::Io(error) => write!(formatter, "读取 yt-dlp 输出失败：{error}"),
            Self::VersionCommandFailed { status, stderr } => {
                write!(
                    formatter,
                    "yt-dlp 版本检查失败（退出码 {:?}）：{}",
                    status,
                    stderr.trim()
                )
            }
            Self::VersionOutputEmpty => write!(formatter, "yt-dlp 版本输出为空。"),
            Self::ProcessFailed { status, stderr } => {
                write!(formatter, "yt-dlp 检索失败（退出码 {:?}）：{}", status, stderr.trim())
            }
            Self::InvalidJson(message) => write!(formatter, "yt-dlp 返回的媒体消息不是有效 JSON：{message}"),
            Self::MissingField(field) => write!(formatter, "yt-dlp 媒体消息缺少字段：{field}。"),
            Self::InvalidField { field, message } => write!(formatter, "yt-dlp 字段 {field} 无效：{message}"),
            Self::Timeout(timeout) => write!(formatter, "yt-dlp 检索超过 {:?}。", timeout),
            Self::Cancelled => write!(formatter, "yt-dlp 检索已取消。"),
            Self::Poisoned => write!(formatter, "yt-dlp 检索任务状态异常。"),
            Self::WorkerPanicked => write!(formatter, "yt-dlp 检索任务异常结束。"),
            Self::InvalidDownloadRequest(message) => write!(formatter, "下载请求无效：{message}"),
            Self::ProgressParse(message) => write!(formatter, "yt-dlp 下载进度无效：{message}"),
            Self::DownloadProcessFailed { status, stderr } => {
                write!(formatter, "yt-dlp 下载失败（退出码 {:?}）：{}", status, stderr.trim())
            }
        }
    }
}

impl std::error::Error for DownloadTaskError {}
