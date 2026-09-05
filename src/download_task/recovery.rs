use std::path::Path;

use crate::storage::{DownloadTaskStatus, DownloadTaskWithStreams, Storage};

use super::{
    DownloadMediaType, DownloadOptions, DownloadProgress, DownloadRequest, DownloadStage, DownloadStreamStatus,
    DownloadTaskError, MediaFormat, StreamProgress, VideoInfo,
};

pub(crate) fn load_request(task_id: i64) -> Result<DownloadRequest, DownloadTaskError> {
    let storage = Storage::instance().map_err(storage_error)?;
    let stored = storage
        .get_download_task(task_id)
        .map_err(storage_error)?
        .ok_or_else(|| DownloadTaskError::Storage("下载任务不存在".to_owned()))?;
    if stored.task.status != DownloadTaskStatus::Paused {
        return Err(DownloadTaskError::InvalidDownloadRequest(
            "只有暂停任务可以继续".to_owned(),
        ));
    }
    let snapshot = storage
        .load_download_execution_snapshot(task_id)
        .map_err(storage_error)?
        .ok_or_else(|| DownloadTaskError::Storage("任务缺少执行快照，无法精确继续".to_owned()))?;
    if stored.streams.len() != 2
        || stored.streams.iter().any(|stream| {
            let expected = match stream.media_type {
                crate::storage::DownloadStreamMediaType::Video => snapshot.video_format_id.as_str(),
                crate::storage::DownloadStreamMediaType::Audio => snapshot.audio_format_id.as_str(),
            };
            stream.format_id.as_deref() != Some(expected)
                || stream.stream_key != expected
                || !matches!(
                    stream.status,
                    DownloadTaskStatus::Paused | DownloadTaskStatus::Completed
                )
        })
    {
        return Err(DownloadTaskError::InvalidDownloadRequest(
            "恢复任务的下载流状态无效".to_owned(),
        ));
    }
    let request = DownloadRequest {
        source_url: snapshot.source_url,
        video: VideoInfo {
            id: stored.task.video_id.unwrap_or_default(),
            title: stored.task.title.unwrap_or_default(),
            webpage_url: None,
            original_url: None,
            uploader: None,
            channel: None,
            duration_seconds: stored.task.duration_seconds.map(|value| value as f64),
            thumbnail_url: stored.task.thumbnail_url,
            description: None,
            upload_date: None,
            formats: stored
                .streams
                .into_iter()
                .map(|stream| MediaFormat {
                    format_id: stream.format_id,
                    format_note: None,
                    extension: stream.extension,
                    resolution: None,
                    width: stream.width.and_then(|value| u64::try_from(value).ok()),
                    height: stream.height.and_then(|value| u64::try_from(value).ok()),
                    fps: None,
                    filesize: stream.total_bytes.and_then(|value| u64::try_from(value).ok()),
                    filesize_approx: stream.total_bytes_estimate.and_then(|value| u64::try_from(value).ok()),
                    bitrate_kbps: None,
                    video_codec: stream.video_codec,
                    audio_codec: stream.audio_codec,
                    audio_bitrate_kbps: None,
                    video_bitrate_kbps: None,
                    protocol: None,
                    url: None,
                })
                .collect(),
        },
        selected_video_format_id: snapshot.video_format_id,
        selected_audio_format_id: snapshot.audio_format_id,
        output_template: snapshot.output_template,
        target_directory: snapshot.target_directory.into(),
        temporary_directory: snapshot.temporary_directory.into(),
        merge_output_format: snapshot.merge_output_format,
        options: DownloadOptions {
            rate_limit: snapshot.options.rate_limit,
            retries: snapshot.options.retries,
            fragment_retries: snapshot.options.fragment_retries,
            file_access_retries: snapshot.options.file_access_retries,
            concurrent_fragments: snapshot.options.concurrent_fragments,
        },
    };
    request.validate().map_err(DownloadTaskError::InvalidDownloadRequest)?;
    validate_directory(&request.target_directory)?;
    validate_directory(&request.temporary_directory)?;
    Ok(request)
}

/// 固定首次执行时的目录，避免重启后当前工作目录变化导致续传位置漂移。
pub(crate) fn prepare_directories(request: &mut DownloadRequest) -> Result<(), DownloadTaskError> {
    for directory in [&mut request.target_directory, &mut request.temporary_directory] {
        if directory.as_os_str().is_empty() {
            return Err(DownloadTaskError::InvalidDownloadRequest("下载目录不能为空".to_owned()));
        }
        *directory = std::path::absolute(&*directory).map_err(DownloadTaskError::Io)?;
        std::fs::create_dir_all(&*directory).map_err(DownloadTaskError::Io)?;
        validate_directory(directory)?;
    }
    Ok(())
}

fn validate_directory(directory: &Path) -> Result<(), DownloadTaskError> {
    if !directory.is_absolute() || !directory.is_dir() {
        return Err(DownloadTaskError::InvalidDownloadRequest(format!(
            "续传目录不存在或不是绝对目录：{}",
            directory.display()
        )));
    }
    std::fs::read_dir(directory).map_err(DownloadTaskError::Io)?;
    Ok(())
}

pub(crate) fn progress(
    task_id: i64,
    stage: DownloadStage,
) -> Result<(DownloadProgress, Vec<StreamProgress>), DownloadTaskError> {
    let stored = Storage::instance()
        .map_err(storage_error)?
        .get_download_task(task_id)
        .map_err(storage_error)?
        .ok_or_else(|| DownloadTaskError::Storage("下载任务不存在".to_owned()))?;
    Ok(progress_from_stored(stored, stage))
}

fn progress_from_stored(
    stored: DownloadTaskWithStreams,
    stage: DownloadStage,
) -> (DownloadProgress, Vec<StreamProgress>) {
    let task = stored.task;
    let progress = DownloadProgress {
        task_id: task.id,
        stage,
        downloaded_bytes: task.downloaded_bytes,
        total_bytes: task.total_bytes,
        total_bytes_estimate: task.total_bytes_estimate,
        speed_bytes_per_second: None,
        elapsed_seconds: None,
        eta_seconds: None,
        percent: task.progress_percent,
        total_is_estimate: task.total_bytes.is_none() && task.total_bytes_estimate.is_some(),
        active_stream: None,
        updated_at: task.updated_at,
    };
    let streams = stored
        .streams
        .into_iter()
        .map(|stream| StreamProgress {
            stream_key: stream.stream_key,
            format_id: stream.format_id,
            media_type: match stream.media_type {
                crate::storage::DownloadStreamMediaType::Video => DownloadMediaType::Video,
                crate::storage::DownloadStreamMediaType::Audio => DownloadMediaType::Audio,
            },
            status: if stream.status == DownloadTaskStatus::Completed {
                DownloadStreamStatus::Finished
            } else {
                DownloadStreamStatus::Downloading
            },
            downloaded_bytes: stream.downloaded_bytes,
            total_bytes: stream.total_bytes,
            total_bytes_estimate: stream.total_bytes_estimate,
            speed_bytes_per_second: None,
            elapsed_seconds: None,
            eta_seconds: None,
            percent: stream.progress_percent,
            started_at: stream.started_at,
            finished_at: stream.finished_at,
        })
        .collect();
    (progress, streams)
}

fn storage_error(error: impl std::fmt::Display) -> DownloadTaskError {
    DownloadTaskError::Storage(error.to_string())
}
