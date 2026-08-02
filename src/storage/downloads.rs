use super::{Storage, timestamp};
use crate::app::state::{DownloadRecord, DownloadStatus, NewDownload};
use crate::error::AppError;
use rusqlite::params;

impl Storage {
    pub fn create_download(&self, download: &NewDownload) -> Result<i64, AppError> {
        let now = timestamp();
        self.connection.execute(
            "INSERT INTO downloads
             (url, resource_name, output_directory, status, format_selector, video_format,
              audio_format, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                download.url,
                download.resource_name,
                download.output_directory,
                DownloadStatus::Queued.as_str(),
                download.format_selector,
                download.video_format,
                download.audio_format,
                now
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn get_download(&self, download_id: i64) -> Result<DownloadRecord, AppError> {
        self.list_downloads()?
            .into_iter()
            .find(|record| record.id == download_id)
            .ok_or_else(|| AppError::NotFound(format!("Download task not found: {download_id}")))
    }

    pub fn list_downloads(&self) -> Result<Vec<DownloadRecord>, AppError> {
        let mut statement = self.connection.prepare(
            "SELECT id, url, resource_name, output_directory, output_path, status,
                    format_selector, video_format, audio_format, downloaded_bytes, total_bytes,
                    started_at, completed_at, COALESCE(error_message, '')
             FROM downloads ORDER BY updated_at DESC, id DESC",
        )?;
        let records = statement.query_map([], |row| {
            let status: String = row.get(5)?;
            Ok(DownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                resource_name: row.get(2)?,
                output_directory: row.get(3)?,
                output_path: row.get(4)?,
                status: DownloadStatus::from_storage(&status),
                format_selector: row.get(6)?,
                video_format: row.get(7)?,
                audio_format: row.get(8)?,
                downloaded_bytes: row.get(9)?,
                total_bytes: row.get(10)?,
                started_at: row.get(11)?,
                completed_at: row.get(12)?,
                error_message: row.get(13)?,
            })
        })?;
        Ok(records.collect::<Result<Vec<_>, _>>()?)
    }

    // Lifecycle writes refresh updated_at because the UI orders tasks by recent activity.
    pub fn mark_started(&self, download_id: i64) -> Result<(), AppError> {
        let now = timestamp();
        self.connection.execute(
            "UPDATE downloads SET status = ?1, started_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![DownloadStatus::Running.as_str(), now, download_id],
        )?;
        Ok(())
    }

    pub fn mark_completed(&self, download_id: i64) -> Result<(), AppError> {
        let now = timestamp();
        self.connection.execute(
            "UPDATE downloads SET status = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![DownloadStatus::Completed.as_str(), now, download_id],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, download_id: i64, message: &str) -> Result<(), AppError> {
        self.connection.execute(
            "UPDATE downloads SET status = ?1, error_message = ?2, updated_at = ?3 WHERE id = ?4",
            params![
                DownloadStatus::Failed.as_str(),
                message,
                timestamp(),
                download_id
            ],
        )?;
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn update_progress(
        &self,
        download_id: i64,
        downloaded_bytes: i64,
        total_bytes: i64,
        output_path: &str,
    ) -> Result<(), AppError> {
        self.connection.execute(
            "UPDATE downloads SET downloaded_bytes = ?1, total_bytes = ?2, output_path = ?3,
             updated_at = ?4 WHERE id = ?5",
            params![
                downloaded_bytes,
                total_bytes,
                output_path,
                timestamp(),
                download_id
            ],
        )?;
        Ok(())
    }

    pub fn append_log(
        &self,
        download_id: i64,
        sequence: i64,
        message: &str,
    ) -> Result<(), AppError> {
        self.connection.execute(
            "INSERT INTO download_logs
             (download_id, sequence, level, message, created_at)
             VALUES (?1, ?2, 'info', ?3, ?4)",
            params![download_id, sequence, message, timestamp()],
        )?;
        Ok(())
    }
}
