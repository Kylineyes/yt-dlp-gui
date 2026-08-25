use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::storage::{
    DownloadProgress as StoredProgress, DownloadStreamMediaType, DownloadTaskDraft, DownloadTaskStatus,
    DownloadTaskStreamDraft, Storage,
};

use super::error::DownloadTaskError;
use super::model::{DownloadMediaType, DownloadProgress, DownloadRequest, MediaFormat, StreamProgress};

const PROGRESS_WRITE_INTERVAL: Duration = Duration::from_millis(750);

pub(crate) struct PersistedDownload {
    storage: &'static Storage,
    task_id: i64,
    stream_ids: HashMap<String, i64>,
    stream_statuses: HashMap<String, DownloadTaskStatus>,
    last_progress_write: Option<Instant>,
    merging: bool,
}

impl PersistedDownload {
    pub(crate) fn create(
        request: &DownloadRequest,
        yt_dlp_version: String,
        now: i64,
    ) -> Result<Self, DownloadTaskError> {
        let storage = Storage::instance().map_err(storage_error)?;
        let video_format = selected_format(&request.video.formats, &request.selected_video_format_id, "视频")?;
        let audio_format = selected_format(&request.video.formats, &request.selected_audio_format_id, "音频")?;
        validate_selected_stream(video_format, DownloadMediaType::Video)?;
        validate_selected_stream(audio_format, DownloadMediaType::Audio)?;
        if request.selected_video_format_id == request.selected_audio_format_id {
            return Err(DownloadTaskError::InvalidDownloadRequest(
                "视频和音频格式 ID 不能相同".to_owned(),
            ));
        }
        let target_path = request
            .target_directory
            .to_str()
            .ok_or_else(|| DownloadTaskError::InvalidDownloadRequest("下载目录不是有效 UTF-8 路径".to_owned()))?
            .to_owned();
        let duration_seconds = request
            .video
            .duration_seconds
            .map(|duration| {
                if !duration.is_finite() || duration < 0.0 || duration > i64::MAX as f64 {
                    return Err(DownloadTaskError::InvalidDownloadRequest("视频时长无效".to_owned()));
                }
                Ok(duration.round() as i64)
            })
            .transpose()?;
        let draft = DownloadTaskDraft {
            source_url: request.source_url.clone(),
            video_id: Some(request.video.id.clone()),
            title: Some(request.video.title.clone()),
            thumbnail_url: request.video.thumbnail_url.clone(),
            duration_seconds,
            target_path,
            output_path: None,
            selected_format: Some(format!(
                "{}+{}",
                request.selected_video_format_id, request.selected_audio_format_id
            )),
            created_at: now,
            yt_dlp_version: Some(yt_dlp_version),
        };
        let streams = vec![
            stream_draft(video_format, DownloadMediaType::Video, now)?,
            stream_draft(audio_format, DownloadMediaType::Audio, now)?,
        ];
        let task = storage.create_download_task(draft, streams).map_err(storage_error)?;
        storage
            .update_download_status(task.id, DownloadTaskStatus::Preparing, now)
            .map_err(storage_error)?;
        let stored = storage
            .get_download_task(task.id)
            .map_err(storage_error)?
            .ok_or_else(|| DownloadTaskError::Storage("刚创建的下载任务无法读取".to_owned()))?;
        let stream_ids = stored
            .streams
            .iter()
            .map(|stream| (stream.stream_key.clone(), stream.id))
            .collect();
        let stream_statuses = stored
            .streams
            .into_iter()
            .map(|stream| (stream.stream_key, stream.status))
            .collect();
        Ok(Self {
            storage,
            task_id: task.id,
            stream_ids,
            stream_statuses,
            last_progress_write: None,
            merging: false,
        })
    }

    pub(crate) fn task_id(&self) -> i64 {
        self.task_id
    }

    pub(crate) fn mark_downloading(&self, now: i64) -> Result<(), DownloadTaskError> {
        self.storage
            .update_download_status(self.task_id, DownloadTaskStatus::Downloading, now)
            .map_err(storage_error)
    }

    pub(crate) fn mark_merging(&mut self, now: i64) -> Result<bool, DownloadTaskError> {
        if self.merging {
            return Ok(false);
        }
        self.storage
            .update_download_status(self.task_id, DownloadTaskStatus::Merging, now)
            .map_err(storage_error)?;
        self.merging = true;
        Ok(true)
    }

    pub(crate) fn write_progress(
        &mut self,
        task_progress: &DownloadProgress,
        stream: &StreamProgress,
        force: bool,
    ) -> Result<(), DownloadTaskError> {
        let stream_id = self.stream_ids.get(&stream.stream_key).copied();
        if let Some(stream_id) = stream_id {
            self.ensure_stream_downloading(&stream.stream_key, stream_id, task_progress.updated_at)?;
        }
        let should_write = force
            || self
                .last_progress_write
                .is_none_or(|last_write| last_write.elapsed() >= PROGRESS_WRITE_INTERVAL);
        if !should_write {
            return Ok(());
        }
        self.last_progress_write = Some(Instant::now());
        let _ = self
            .storage
            .update_download_progress(self.task_id, stored_task_progress(task_progress));
        if let Some(stream_id) = stream_id {
            let _ = self
                .storage
                .update_download_stream_progress(stream_id, stored_stream_progress(stream, task_progress.updated_at));
        }
        if stream.status == super::model::DownloadStreamStatus::Finished {
            if let Some(stream_id) = stream_id {
                self.complete_stream(&stream.stream_key, stream_id, task_progress.updated_at)?;
            }
        }
        Ok(())
    }

    fn ensure_stream_downloading(
        &mut self,
        stream_key: &str,
        stream_id: i64,
        now: i64,
    ) -> Result<(), DownloadTaskError> {
        let status = self
            .stream_statuses
            .get(stream_key)
            .copied()
            .ok_or_else(|| DownloadTaskError::Storage(format!("未找到下载流映射：{stream_key}")))?;
        match status {
            DownloadTaskStatus::Pending => {
                self.storage
                    .update_download_stream_status(stream_id, DownloadTaskStatus::Preparing, now)
                    .map_err(storage_error)?;
                self.stream_statuses
                    .insert(stream_key.to_owned(), DownloadTaskStatus::Preparing);
                self.storage
                    .update_download_stream_status(stream_id, DownloadTaskStatus::Downloading, now)
                    .map_err(storage_error)?;
                self.stream_statuses
                    .insert(stream_key.to_owned(), DownloadTaskStatus::Downloading);
            }
            DownloadTaskStatus::Preparing => {
                self.storage
                    .update_download_stream_status(stream_id, DownloadTaskStatus::Downloading, now)
                    .map_err(storage_error)?;
                self.stream_statuses
                    .insert(stream_key.to_owned(), DownloadTaskStatus::Downloading);
            }
            DownloadTaskStatus::Downloading => {}
            DownloadTaskStatus::Completed | DownloadTaskStatus::Cancelled | DownloadTaskStatus::Failed => {
                return Err(DownloadTaskError::Storage(format!("下载流已处于终态：{stream_key}")))
            }
            DownloadTaskStatus::Merging => {
                return Err(DownloadTaskError::Storage(format!("下载流状态无效：{stream_key}")))
            }
        }
        Ok(())
    }

    fn complete_stream(&mut self, stream_key: &str, stream_id: i64, now: i64) -> Result<(), DownloadTaskError> {
        match self.stream_statuses.get(stream_key).copied() {
            Some(DownloadTaskStatus::Downloading) => {
                self.storage
                    .complete_download_stream(stream_id, now)
                    .map_err(storage_error)?;
                self.stream_statuses
                    .insert(stream_key.to_owned(), DownloadTaskStatus::Completed);
                Ok(())
            }
            Some(DownloadTaskStatus::Completed) => Ok(()),
            Some(status) => Err(DownloadTaskError::Storage(format!(
                "下载流无法完成，当前状态为 {status:?}：{stream_key}"
            ))),
            None => Err(DownloadTaskError::Storage(format!("未找到下载流映射：{stream_key}"))),
        }
    }

    pub(crate) fn write_final_progress(&mut self, task_progress: &DownloadProgress, streams: &[StreamProgress]) {
        let _ = self
            .storage
            .update_download_progress(self.task_id, stored_task_progress(task_progress));
        for stream in streams {
            if let Some(stream_id) = self.stream_ids.get(&stream.stream_key) {
                let _ = self.storage.update_download_stream_progress(
                    *stream_id,
                    stored_stream_progress(stream, task_progress.updated_at),
                );
            }
        }
        self.last_progress_write = Some(Instant::now());
    }

    pub(crate) fn complete(&self, output_path: String, now: i64) -> Result<(), DownloadTaskError> {
        self.storage
            .complete_download_task(self.task_id, output_path, now)
            .map_err(storage_error)
    }

    pub(crate) fn cancel(&mut self, now: i64) -> Result<(), DownloadTaskError> {
        self.finish_active_streams(DownloadTaskStatus::Cancelled, now)?;
        self.storage
            .cancel_download_task(self.task_id, now)
            .map_err(storage_error)
    }

    pub(crate) fn fail(&mut self, error: &DownloadTaskError, now: i64) -> Result<(), DownloadTaskError> {
        self.finish_active_streams(DownloadTaskStatus::Failed, now)?;
        self.storage
            .fail_download_task(
                self.task_id,
                Some(error_code(error).to_owned()),
                limited_error_message(error),
                now,
            )
            .map_err(storage_error)
    }

    fn finish_active_streams(&mut self, target: DownloadTaskStatus, now: i64) -> Result<(), DownloadTaskError> {
        let active_streams = self
            .stream_statuses
            .iter()
            .filter_map(|(stream_key, status)| (!status.is_terminal()).then(|| stream_key.clone()))
            .collect::<Vec<_>>();
        for stream_key in active_streams {
            let stream_id = self
                .stream_ids
                .get(&stream_key)
                .copied()
                .ok_or_else(|| DownloadTaskError::Storage(format!("未找到下载流映射：{stream_key}")))?;
            if matches!(target, DownloadTaskStatus::Cancelled) {
                self.storage
                    .cancel_download_stream(stream_id, now)
                    .map_err(storage_error)?;
            } else if matches!(target, DownloadTaskStatus::Failed) {
                self.storage
                    .fail_download_stream(stream_id, now)
                    .map_err(storage_error)?;
            }
            self.stream_statuses.insert(stream_key, target);
        }
        Ok(())
    }
}

fn selected_format<'a>(
    formats: &'a [MediaFormat],
    format_id: &str,
    label: &str,
) -> Result<&'a MediaFormat, DownloadTaskError> {
    formats
        .iter()
        .find(|format| format.format_id.as_deref() == Some(format_id))
        .ok_or_else(|| DownloadTaskError::InvalidDownloadRequest(format!("未找到所选{label}格式：{format_id}")))
}

fn validate_selected_stream(format: &MediaFormat, media_type: DownloadMediaType) -> Result<(), DownloadTaskError> {
    let (required_codec, forbidden_codec, missing_message, combined_message) = match media_type {
        DownloadMediaType::Video => (
            format.video_codec.as_deref(),
            format.audio_codec.as_deref(),
            "所选视频格式不包含视频流",
            "所选视频格式必须是独立视频流",
        ),
        DownloadMediaType::Audio => (
            format.audio_codec.as_deref(),
            format.video_codec.as_deref(),
            "所选音频格式不包含音频流",
            "所选音频格式必须是独立音频流",
        ),
    };
    if required_codec.is_none_or(|codec| codec.is_empty() || codec == "none") {
        return Err(DownloadTaskError::InvalidDownloadRequest(missing_message.to_owned()));
    }
    if forbidden_codec.is_some_and(|codec| !codec.is_empty() && codec != "none") {
        return Err(DownloadTaskError::InvalidDownloadRequest(combined_message.to_owned()));
    }
    Ok(())
}

fn stream_draft(
    format: &MediaFormat,
    media_type: DownloadMediaType,
    now: i64,
) -> Result<DownloadTaskStreamDraft, DownloadTaskError> {
    let format_id = format
        .format_id
        .clone()
        .ok_or_else(|| DownloadTaskError::InvalidDownloadRequest("所选格式缺少格式 ID".to_owned()))?;
    Ok(DownloadTaskStreamDraft {
        stream_key: format_id.clone(),
        format_id: Some(format_id),
        media_type: match media_type {
            DownloadMediaType::Video => DownloadStreamMediaType::Video,
            DownloadMediaType::Audio => DownloadStreamMediaType::Audio,
        },
        extension: format.extension.clone(),
        width: optional_u64_to_i64(format.width, "视频宽度")?,
        height: optional_u64_to_i64(format.height, "视频高度")?,
        video_codec: format.video_codec.clone(),
        audio_codec: format.audio_codec.clone(),
        created_at: now,
    })
}

fn optional_u64_to_i64(value: Option<u64>, label: &str) -> Result<Option<i64>, DownloadTaskError> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| DownloadTaskError::InvalidDownloadRequest(format!("{label}超出支持范围")))
        })
        .transpose()
}

fn stored_task_progress(progress: &DownloadProgress) -> StoredProgress {
    StoredProgress {
        progress_percent: progress.percent,
        downloaded_bytes: progress.downloaded_bytes,
        total_bytes: progress.total_bytes,
        total_bytes_estimate: progress.total_bytes_estimate,
        speed_bytes_per_second: progress.speed_bytes_per_second,
        elapsed_seconds: progress.elapsed_seconds,
        eta_seconds: progress.eta_seconds,
        updated_at: progress.updated_at,
    }
}

fn stored_stream_progress(progress: &StreamProgress, updated_at: i64) -> StoredProgress {
    StoredProgress {
        progress_percent: progress.percent,
        downloaded_bytes: progress.downloaded_bytes,
        total_bytes: progress.total_bytes,
        total_bytes_estimate: progress.total_bytes_estimate,
        speed_bytes_per_second: progress.speed_bytes_per_second,
        elapsed_seconds: progress.elapsed_seconds,
        eta_seconds: progress.eta_seconds,
        updated_at,
    }
}

fn error_code(error: &DownloadTaskError) -> &'static str {
    match error {
        DownloadTaskError::ExecutableNotFound(_) => "executable_not_found",
        DownloadTaskError::Spawn(_) => "spawn_failed",
        DownloadTaskError::Io(_) => "io_failed",
        DownloadTaskError::Timeout(_) => "timeout",
        DownloadTaskError::Cancelled => "cancelled",
        DownloadTaskError::ProgressParse(_) => "progress_parse_failed",
        DownloadTaskError::DownloadProcessFailed { .. } => "download_process_failed",
        DownloadTaskError::OutputPathMissing => "output_path_missing",
        DownloadTaskError::Storage(_) => "storage_failed",
        _ => "download_failed",
    }
}

fn limited_error_message(error: &DownloadTaskError) -> String {
    const MAX_CHARS: usize = 4096;
    error.to_string().chars().take(MAX_CHARS).collect()
}

fn storage_error(error: impl std::fmt::Display) -> DownloadTaskError {
    DownloadTaskError::Storage(error.to_string())
}
