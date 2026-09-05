use super::error::StorageError;

/// 下载任务在持久化层允许出现的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadTaskStatus {
    Pending,
    Preparing,
    Downloading,
    Paused,
    Merging,
    Completed,
    Cancelled,
    Failed,
}

impl DownloadTaskStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Preparing => "preparing",
            Self::Downloading => "downloading",
            Self::Paused => "paused",
            Self::Merging => "merging",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn parse(value: String) -> Result<Self, StorageError> {
        match value.as_str() {
            "pending" => Ok(Self::Pending),
            "preparing" => Ok(Self::Preparing),
            "downloading" => Ok(Self::Downloading),
            "paused" => Ok(Self::Paused),
            "merging" => Ok(Self::Merging),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            _ => Err(StorageError::InvalidStoredDownloadStatus(value)),
        }
    }

    pub(crate) const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Preparing | Self::Cancelled | Self::Failed)
                | (
                    Self::Preparing,
                    Self::Downloading | Self::Paused | Self::Cancelled | Self::Failed
                )
                | (
                    Self::Downloading,
                    Self::Paused | Self::Merging | Self::Completed | Self::Cancelled | Self::Failed
                )
                | (Self::Paused, Self::Preparing | Self::Cancelled | Self::Failed)
                | (Self::Merging, Self::Completed | Self::Failed)
        )
    }

    /// 下载流不参与任务级合并阶段，只允许下载阶段、暂停和三个终态。
    pub(crate) const fn can_stream_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Pending, Self::Preparing | Self::Cancelled | Self::Failed)
                | (
                    Self::Preparing,
                    Self::Downloading | Self::Paused | Self::Cancelled | Self::Failed
                )
                | (
                    Self::Downloading,
                    Self::Paused | Self::Completed | Self::Cancelled | Self::Failed
                )
                | (Self::Paused, Self::Preparing | Self::Cancelled | Self::Failed)
        )
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    pub(crate) const fn can_accept_progress(self) -> bool {
        !self.is_terminal() && !matches!(self, Self::Paused)
    }
}

/// 下载流的媒体类型；临时媒体 URL 不属于持久化模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStreamMediaType {
    Video,
    Audio,
}

impl DownloadStreamMediaType {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Audio => "audio",
        }
    }

    pub(crate) fn parse(value: String) -> Result<Self, StorageError> {
        match value.as_str() {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            _ => Err(StorageError::InvalidStoredMediaType(value)),
        }
    }
}

/// 创建下载任务时写入的元数据快照，不包含命令行或代理认证信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTaskDraft {
    pub source_url: String,
    pub video_id: Option<String>,
    pub title: Option<String>,
    pub thumbnail_url: Option<String>,
    pub duration_seconds: Option<i64>,
    pub target_path: String,
    pub output_path: Option<String>,
    pub selected_format: Option<String>,
    pub created_at: i64,
    pub yt_dlp_version: Option<String>,
}

/// 下载命令中与单个任务绑定的请求级选项快照。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DownloadExecutionOptions {
    pub rate_limit: Option<String>,
    pub retries: Option<u32>,
    pub fragment_retries: Option<u32>,
    pub file_access_retries: Option<u32>,
    pub concurrent_fragments: Option<u32>,
}

/// 任务创建后不可变的执行参数；恢复时必须只读取此快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadExecutionSnapshot {
    pub source_url: String,
    pub video_format_id: String,
    pub audio_format_id: String,
    pub output_template: String,
    pub target_directory: String,
    pub temporary_directory: String,
    pub merge_output_format: String,
    pub options: DownloadExecutionOptions,
}

/// 创建下载流时写入的格式和媒体属性快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTaskStreamDraft {
    pub stream_key: String,
    pub format_id: Option<String>,
    pub media_type: DownloadStreamMediaType,
    pub extension: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub created_at: i64,
}

/// 任务或流的增量进度快照；未知数值使用 `None` 表示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub progress_percent: Option<u8>,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub total_bytes_estimate: Option<i64>,
    pub speed_bytes_per_second: Option<i64>,
    pub elapsed_seconds: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub updated_at: i64,
}

pub type DownloadStreamProgress = DownloadProgress;

/// 数据库中的下载任务完整快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    pub id: i64,
    pub source_url: String,
    pub video_id: Option<String>,
    pub title: Option<String>,
    pub thumbnail_url: Option<String>,
    pub duration_seconds: Option<i64>,
    pub target_path: String,
    pub output_path: Option<String>,
    pub selected_format: Option<String>,
    pub status: DownloadTaskStatus,
    pub progress_percent: Option<u8>,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub total_bytes_estimate: Option<i64>,
    pub speed_bytes_per_second: Option<i64>,
    pub elapsed_seconds: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
    pub yt_dlp_version: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

/// 数据库中的单个下载流完整快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTaskStream {
    pub id: i64,
    pub task_id: i64,
    pub stream_key: String,
    pub format_id: Option<String>,
    pub media_type: DownloadStreamMediaType,
    pub extension: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub status: DownloadTaskStatus,
    pub progress_percent: Option<u8>,
    pub downloaded_bytes: i64,
    pub total_bytes: Option<i64>,
    pub total_bytes_estimate: Option<i64>,
    pub speed_bytes_per_second: Option<i64>,
    pub elapsed_seconds: Option<i64>,
    pub eta_seconds: Option<i64>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
}

/// 任务快照及其关联的全部流快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTaskWithStreams {
    pub task: DownloadTask,
    pub streams: Vec<DownloadTaskStream>,
}

/// 任务列表的可选状态筛选和数量上限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DownloadTaskFilter {
    pub status: Option<DownloadTaskStatus>,
    pub limit: Option<u32>,
}

impl DownloadTaskDraft {
    pub(crate) fn validate(&self) -> Result<(), StorageError> {
        if self.source_url.is_empty() || self.target_path.is_empty() || self.created_at < 0 {
            return Err(StorageError::InvalidDownloadInput);
        }
        validate_optional_nonnegative(self.duration_seconds)
    }
}

impl DownloadExecutionSnapshot {
    pub(crate) fn validate_stream(&self, stream: &DownloadTaskStreamDraft) -> Result<(), StorageError> {
        let expected = match stream.media_type {
            DownloadStreamMediaType::Video => &self.video_format_id,
            DownloadStreamMediaType::Audio => &self.audio_format_id,
        };
        if stream.format_id.as_ref() != Some(expected) {
            return Err(StorageError::InvalidDownloadInput);
        }
        Ok(())
    }

    pub(crate) fn validate_for_task(&self, task: &DownloadTaskDraft) -> Result<(), StorageError> {
        if self.source_url != task.source_url
            || self.target_directory != task.target_path
            || self.source_url.trim().is_empty()
            || self.video_format_id.trim().is_empty()
            || self.audio_format_id.trim().is_empty()
            || self.video_format_id == self.audio_format_id
            || task
                .selected_format
                .as_ref()
                .is_some_and(|format| *format != format!("{}+{}", self.video_format_id, self.audio_format_id))
            || self.output_template.trim().is_empty()
            || self.target_directory.trim().is_empty()
            || self.temporary_directory.trim().is_empty()
            || !matches!(self.merge_output_format.as_str(), "mp4" | "mkv")
        {
            return Err(StorageError::InvalidDownloadInput);
        }
        Ok(())
    }
}

impl DownloadTaskStreamDraft {
    pub(crate) fn validate(&self) -> Result<(), StorageError> {
        if self.stream_key.is_empty() || self.created_at < 0 {
            return Err(StorageError::InvalidDownloadInput);
        }
        validate_optional_nonnegative(self.width)?;
        validate_optional_nonnegative(self.height)
    }
}

impl DownloadProgress {
    pub(crate) fn validate(self) -> Result<(), StorageError> {
        if self.updated_at < 0 || self.downloaded_bytes < 0 {
            return Err(StorageError::InvalidDownloadProgress);
        }
        for value in [
            self.total_bytes,
            self.total_bytes_estimate,
            self.speed_bytes_per_second,
            self.elapsed_seconds,
            self.eta_seconds,
        ] {
            validate_optional_nonnegative(value)?;
        }
        Ok(())
    }
}

fn validate_optional_nonnegative(value: Option<i64>) -> Result<(), StorageError> {
    if value.is_some_and(|value| value < 0) {
        return Err(StorageError::InvalidDownloadInput);
    }
    Ok(())
}
