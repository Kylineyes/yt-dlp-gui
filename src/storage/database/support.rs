use rusqlite::{Result, Row};

use super::super::download::{DownloadStreamMediaType, DownloadTask, DownloadTaskStatus, DownloadTaskStream};
use super::super::error::StorageError;

pub(crate)const TASK_SELECT: &str = "SELECT id, source_url, video_id, title, thumbnail_url, duration_seconds, target_path, output_path, selected_format, status, progress_percent, downloaded_bytes, total_bytes, total_bytes_estimate, speed_bytes_per_second, elapsed_seconds, eta_seconds, created_at, started_at, finished_at, updated_at, yt_dlp_version, error_code, error_message FROM download_tasks";
pub(crate)const STREAM_SELECT: &str = "SELECT id, task_id, stream_key, format_id, media_type, extension, width, height, video_codec, audio_codec, status, progress_percent, downloaded_bytes, total_bytes, total_bytes_estimate, speed_bytes_per_second, elapsed_seconds, eta_seconds, created_at, started_at, finished_at, updated_at FROM download_task_streams WHERE task_id = ?1 ORDER BY id";

pub(crate) fn map_task(row: &Row<'_>) -> Result<DownloadTask> {
    Ok(DownloadTask {
        id: row.get(0)?,
        source_url: row.get(1)?,
        video_id: row.get(2)?,
        title: row.get(3)?,
        thumbnail_url: row.get(4)?,
        duration_seconds: row.get(5)?,
        target_path: row.get(6)?,
        output_path: row.get(7)?,
        selected_format: row.get(8)?,
        status: DownloadTaskStatus::parse(row.get(9)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
        progress_percent: row.get::<_, Option<i64>>(10)?.map(|v| v as u8),
        downloaded_bytes: row.get(11)?,
        total_bytes: row.get(12)?,
        total_bytes_estimate: row.get(13)?,
        speed_bytes_per_second: row.get(14)?,
        elapsed_seconds: row.get(15)?,
        eta_seconds: row.get(16)?,
        created_at: row.get(17)?,
        started_at: row.get(18)?,
        finished_at: row.get(19)?,
        updated_at: row.get(20)?,
        yt_dlp_version: row.get(21)?,
        error_code: row.get(22)?,
        error_message: row.get(23)?,
    })
}

pub(crate) fn map_stream(row: &Row<'_>) -> Result<DownloadTaskStream> {
    Ok(DownloadTaskStream {
        id: row.get(0)?,
        task_id: row.get(1)?,
        stream_key: row.get(2)?,
        format_id: row.get(3)?,
        media_type: DownloadStreamMediaType::parse(row.get(4)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
        extension: row.get(5)?,
        width: row.get(6)?,
        height: row.get(7)?,
        video_codec: row.get(8)?,
        audio_codec: row.get(9)?,
        status: DownloadTaskStatus::parse(row.get(10)?).map_err(|_| rusqlite::Error::InvalidQuery)?,
        progress_percent: row.get::<_, Option<i64>>(11)?.map(|v| v as u8),
        downloaded_bytes: row.get(12)?,
        total_bytes: row.get(13)?,
        total_bytes_estimate: row.get(14)?,
        speed_bytes_per_second: row.get(15)?,
        elapsed_seconds: row.get(16)?,
        eta_seconds: row.get(17)?,
        created_at: row.get(18)?,
        started_at: row.get(19)?,
        finished_at: row.get(20)?,
        updated_at: row.get(21)?,
    })
}

pub(crate) fn map_download_write_error(error: rusqlite::Error) -> StorageError {
    match error {
        rusqlite::Error::SqliteFailure(_, _) => StorageError::Write(error),
        _ => StorageError::Write(error),
    }
}
