use crate::app::state::{AppSettings, DownloadRecord, DownloadStatus, NewDownload};
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Storage {
    connection: Connection,
}

impl Storage {
    pub fn open_default() -> Result<Self, AppError> {
        let base_directory = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let application_directory = base_directory.join("yt-dlp-gui");
        std::fs::create_dir_all(&application_directory)?;
        Self::open(application_directory.join("application.sqlite3"))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS app_config (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS downloads (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 url TEXT NOT NULL,
                 output_directory TEXT NOT NULL,
                 status TEXT NOT NULL,
                 error_message TEXT,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS download_logs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 download_id INTEGER NOT NULL,
                 sequence INTEGER NOT NULL,
                 level TEXT NOT NULL,
                 message TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 FOREIGN KEY (download_id) REFERENCES downloads(id) ON DELETE CASCADE
             );",
        )?;
        let storage = Self { connection };
        storage.migrate()?;
        Ok(storage)
    }

    fn migrate(&self) -> Result<(), AppError> {
        for (column, definition) in [
            ("resource_name", "TEXT NOT NULL DEFAULT ''"),
            ("format_selector", "TEXT NOT NULL DEFAULT ''"),
            ("video_format", "TEXT NOT NULL DEFAULT ''"),
            ("audio_format", "TEXT NOT NULL DEFAULT ''"),
            ("output_path", "TEXT NOT NULL DEFAULT ''"),
            ("downloaded_bytes", "INTEGER NOT NULL DEFAULT 0"),
            ("total_bytes", "INTEGER NOT NULL DEFAULT 0"),
            ("started_at", "INTEGER"),
            ("completed_at", "INTEGER"),
        ] {
            if !self.has_download_column(column)? {
                self.connection.execute(
                    &format!("ALTER TABLE downloads ADD COLUMN {column} {definition}"),
                    [],
                )?;
            }
        }
        self.connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_download_logs_download_id
                 ON download_logs(download_id, sequence);
             CREATE INDEX IF NOT EXISTS idx_downloads_updated_at
                 ON downloads(updated_at DESC, id DESC);
             PRAGMA user_version = 1;",
        )?;
        Ok(())
    }

    fn has_download_column(&self, column: &str) -> Result<bool, AppError> {
        let mut statement = self.connection.prepare("PRAGMA table_info(downloads)")?;
        let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
        for result in columns {
            if result? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn load_settings(&self) -> Result<AppSettings, AppError> {
        let defaults = AppSettings::default();
        Ok(AppSettings {
            yt_dlp_path: self
                .config_value("yt_dlp_path")?
                .unwrap_or(defaults.yt_dlp_path),
            ffmpeg_path: self
                .config_value("ffmpeg_path")?
                .unwrap_or(defaults.ffmpeg_path),
            default_download_directory: self
                .config_value("default_download_directory")?
                .unwrap_or(defaults.default_download_directory),
            proxy: self.config_value("proxy")?.unwrap_or(defaults.proxy),
            max_concurrency: self
                .config_value("max_concurrency")?
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.max_concurrency)
                .clamp(1, 16),
            language: self.config_value("language")?.unwrap_or(defaults.language),
        })
    }

    fn config_value(&self, key: &str) -> Result<Option<String>, AppError> {
        Ok(self
            .connection
            .query_row(
                "SELECT value FROM app_config WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        let now = timestamp();
        for (key, value) in [
            ("yt_dlp_path", settings.yt_dlp_path.clone()),
            ("ffmpeg_path", settings.ffmpeg_path.clone()),
            (
                "default_download_directory",
                settings.default_download_directory.clone(),
            ),
            ("proxy", settings.proxy.clone()),
            ("max_concurrency", settings.max_concurrency.to_string()),
            ("language", settings.language.clone()),
        ] {
            self.connection.execute(
                "INSERT INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, value, now],
            )?;
        }
        Ok(())
    }

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

fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::Storage;
    use crate::app::state::{AppSettings, NewDownload};

    #[test]
    fn persists_settings_and_download_progress() {
        let storage = Storage::open(":memory:").expect("in-memory database should open");
        let settings = AppSettings {
            proxy: "http://127.0.0.1:8080".into(),
            max_concurrency: 3,
            ..AppSettings::default()
        };
        storage
            .save_settings(&settings)
            .expect("settings should be saved");
        assert_eq!(
            storage
                .load_settings()
                .expect("settings should be loaded")
                .max_concurrency,
            3
        );

        let id = storage
            .create_download(&NewDownload {
                url: "https://example.com/video".into(),
                resource_name: "Test resource".into(),
                output_directory: "downloads".into(),
                format_selector: "best".into(),
                video_format: "H.264".into(),
                audio_format: "AAC".into(),
            })
            .expect("download record should be created");
        storage.mark_started(id).expect("task should start");
        storage
            .update_progress(id, 1024, 2048, "downloads/test.mp4")
            .expect("progress should be updated");
        storage
            .append_log(id, 1, "test log")
            .expect("log should be written");
        storage.mark_completed(id).expect("task should complete");
        let records = storage.list_downloads().expect("tasks should be listed");
        assert_eq!(records[0].downloaded_bytes, 1024);
        assert_eq!(records[0].resource_name, "Test resource");
    }
}
