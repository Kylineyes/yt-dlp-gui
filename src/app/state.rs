#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    Queued,
    Ready,
    Running,
    Paused,
    Completed,
    Failed,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "ready" => Self::Ready,
            "running" => Self::Running,
            "paused" => Self::Paused,
            "completed" | "succeeded" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Queued,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    FollowSystem,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::FollowSystem => "system",
        }
    }

    pub fn from_storage(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::FollowSystem,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub yt_dlp_path: String,
    pub ffmpeg_path: String,
    pub default_download_directory: String,
    pub proxy: String,
    pub max_concurrency: u32,
    pub language: String,
    pub theme: ThemeMode,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            yt_dlp_path: String::new(),
            ffmpeg_path: String::new(),
            default_download_directory: String::new(),
            proxy: String::new(),
            max_concurrency: 1,
            language: "zh-CN".into(),
            theme: ThemeMode::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaStream {
    pub id: String,
    pub label: String,
    pub format_selector: String,
    pub video_format: String,
    pub audio_format: String,
    pub estimated_size: String,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub resource_name: String,
    pub streams: Vec<MediaStream>,
}

#[derive(Debug, Clone)]
pub struct NewDownload {
    pub url: String,
    pub resource_name: String,
    pub output_directory: String,
    pub format_selector: String,
    pub video_format: String,
    pub audio_format: String,
}

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub resource_name: String,
    pub output_directory: String,
    pub output_path: String,
    pub status: DownloadStatus,
    pub format_selector: String,
    pub video_format: String,
    pub audio_format: String,
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub error_message: String,
}
