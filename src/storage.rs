mod app_config;
mod config_keys;
mod schema;

use self::app_config::AppConfigStore;
use crate::app::state::{AppSettings, DownloadRecord, DownloadStatus, NewDownload};
use crate::error::{AppError, StorageStage};
use rusqlite::Connection;
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DATABASE_FILE_NAME: &str = "application.sqlite3";

pub struct Storage {
    connection: Connection,
}

impl Storage {
    /// Opens or creates the application database beside the running executable.
    ///
    /// This is the startup integration point for the application worker.
    pub fn open_default() -> Result<Self, AppError> {
        let executable_path = std::env::current_exe().map_err(|source| AppError::StorageIo {
            stage: StorageStage::ResolveExecutablePath,
            path: None,
            source,
        })?;
        let database_path = database_path_next_to_executable(&executable_path)?;
        Self::open_or_initialize(database_path)
    }

    /// Opens or creates a database and ensures its schema is ready for use.
    pub fn open_or_initialize(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let database_path = path.as_ref().to_path_buf();
        if let Some(parent) = database_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|source| AppError::StorageIo {
                stage: StorageStage::CreateDatabaseDirectory,
                path: Some(parent.to_path_buf()),
                source,
            })?;
        }
        let connection =
            Connection::open(&database_path).map_err(|source| AppError::StorageSqlite {
                stage: StorageStage::OpenDatabase,
                path: database_path.clone(),
                source,
            })?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|source| AppError::StorageSqlite {
                stage: StorageStage::ConfigureConnection,
                path: database_path.clone(),
                source,
            })?;
        Self::configure_connection(&connection).map_err(|source| AppError::StorageSqlite {
            stage: StorageStage::ConfigureConnection,
            path: database_path.clone(),
            source,
        })?;
        schema::create_tables(&connection).map_err(|source| AppError::StorageSqlite {
            stage: StorageStage::CreateTables,
            path: database_path.clone(),
            source,
        })?;

        let storage = Self { connection };
        storage
            .migrate()
            .map_err(|source| AppError::StorageSqlite {
                stage: StorageStage::MigrateSchema,
                path: database_path,
                source,
            })?;
        Ok(storage)
    }

    fn configure_connection(connection: &Connection) -> rusqlite::Result<()> {
        const ATTEMPTS: usize = 10;
        for attempt in 0..ATTEMPTS {
            match connection.execute_batch(
                "PRAGMA foreign_keys = ON;
                 PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;",
            ) {
                Ok(()) => return Ok(()),
                Err(error)
                    if matches!(
                        &error,
                        rusqlite::Error::SqliteFailure(sqlite_error, _)
                            if matches!(
                                sqlite_error.code,
                                rusqlite::ErrorCode::DatabaseBusy
                                    | rusqlite::ErrorCode::DatabaseLocked
                            )
                    ) && attempt + 1 < ATTEMPTS =>
                {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("connection configuration attempts should return")
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        // Serialize schema inspection and updates across concurrent application instances.
        self.connection.execute_batch("BEGIN IMMEDIATE;")?;
        let migration_result = self.migrate_schema();
        match migration_result {
            Ok(()) => self.connection.execute_batch("COMMIT;"),
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK;");
                Err(error)
            }
        }
    }

    fn migrate_schema(&self) -> rusqlite::Result<()> {
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
                 ON downloads(updated_at DESC, id DESC);",
        )?;
        AppConfigStore::new(&self.connection)
            .seed_defaults(&AppSettings::default(), timestamp())?;
        self.connection.execute_batch("PRAGMA user_version = 1;")?;
        Ok(())
    }

    fn has_download_column(&self, column: &str) -> rusqlite::Result<bool> {
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
        Ok(AppConfigStore::new(&self.connection).load_settings()?)
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        AppConfigStore::new(&self.connection).save_settings(settings, timestamp())?;
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

fn database_path_next_to_executable(executable_path: &Path) -> Result<PathBuf, AppError> {
    let executable_directory = executable_path
        .parent()
        .ok_or_else(|| AppError::StorageIo {
            stage: StorageStage::ResolveExecutablePath,
            path: Some(executable_path.to_path_buf()),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Executable path has no parent directory",
            ),
        })?;
    Ok(executable_directory.join(DATABASE_FILE_NAME))
}

fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::config_keys::ALL_CONFIG_KEYS;
    use super::{DATABASE_FILE_NAME, Storage, database_path_next_to_executable};
    use crate::app::state::{AppSettings, NewDownload};
    use crate::error::{AppError, StorageStage};
    use std::error::Error;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn create(label: &str) -> Self {
            let unique = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "yt-dlp-gui-{label}-{}-{timestamp}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("test directory should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn persists_settings_and_download_progress() {
        let storage = Storage::open_or_initialize(":memory:")
            .expect("in-memory database should open and initialize");
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

    #[test]
    fn initializes_app_config_with_default_values() {
        let storage = Storage::open_or_initialize(":memory:")
            .expect("in-memory database should initialize defaults");
        let defaults = AppSettings::default();

        assert_eq!(config_row_count(&storage), ALL_CONFIG_KEYS.len());
        for key in ALL_CONFIG_KEYS {
            let (value, updated_at) = config_entry(&storage, key.as_str());
            assert_eq!(value, key.value_from(&defaults));
            assert!(updated_at >= 0);
        }
        assert_eq!(
            storage
                .load_settings()
                .expect("default settings should load"),
            defaults
        );
        assert!(defaults.default_download_directory.is_empty());
    }

    #[test]
    fn initialization_preserves_existing_config_and_fills_missing_keys() {
        let test_directory = TestDirectory::create("partial-config");
        let database_path = test_directory.path().join(DATABASE_FILE_NAME);
        {
            let storage = Storage::open_or_initialize(&database_path)
                .expect("database should initialize defaults");
            storage
                .connection
                .execute("DELETE FROM app_config", [])
                .expect("default config should be cleared");
            storage
                .connection
                .execute(
                    "INSERT INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)",
                    rusqlite::params![super::config_keys::PROXY, "http://127.0.0.1:7890", 42],
                )
                .expect("custom proxy should be inserted");
            storage
                .connection
                .execute(
                    "INSERT INTO app_config (key, value, updated_at) VALUES ('future_key', 'future', 7)",
                    [],
                )
                .expect("unknown key should be inserted");
        }

        let storage = Storage::open_or_initialize(&database_path)
            .expect("missing config values should be seeded");
        assert_eq!(
            config_entry(&storage, super::config_keys::PROXY),
            ("http://127.0.0.1:7890".into(), 42)
        );
        assert_eq!(config_entry(&storage, "future_key"), ("future".into(), 7));
        assert_eq!(config_row_count(&storage), ALL_CONFIG_KEYS.len() + 1);
    }

    #[test]
    fn creates_database_file_and_initializes_schema() {
        let test_directory = TestDirectory::create("initialize");
        let database_path = test_directory.path().join(DATABASE_FILE_NAME);
        assert!(!database_path.exists());

        let storage = Storage::open_or_initialize(&database_path)
            .expect("database file and schema should be initialized");
        assert!(database_path.is_file());

        let tables = object_names(&storage, "table");
        for table in ["app_config", "downloads", "download_logs"] {
            assert!(tables.iter().any(|name| name == table));
        }

        let columns = download_columns(&storage);
        for column in [
            "resource_name",
            "format_selector",
            "video_format",
            "audio_format",
            "output_path",
            "downloaded_bytes",
            "total_bytes",
            "started_at",
            "completed_at",
        ] {
            assert!(columns.iter().any(|name| name == column));
        }

        let indexes = object_names(&storage, "index");
        for index in ["idx_download_logs_download_id", "idx_downloads_updated_at"] {
            assert!(indexes.iter().any(|name| name == index));
        }
        let schema_version: i64 = storage
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("schema version should be readable");
        assert_eq!(schema_version, 1);
    }

    #[test]
    fn initialization_is_idempotent_and_preserves_data() {
        let test_directory = TestDirectory::create("idempotent");
        let database_path = test_directory.path().join(DATABASE_FILE_NAME);
        {
            let storage = Storage::open_or_initialize(&database_path)
                .expect("database should initialize the first time");
            let settings = AppSettings {
                proxy: "http://127.0.0.1:8080".into(),
                max_concurrency: 7,
                language: "en".into(),
                ..AppSettings::default()
            };
            storage
                .save_settings(&settings)
                .expect("settings should be saved");
        }

        let stored_before_reopen = config_entries(&database_path);
        let reopened = Storage::open_or_initialize(&database_path)
            .expect("initialized database should reopen");
        let settings = reopened
            .load_settings()
            .expect("existing settings should be loaded");
        assert_eq!(settings.max_concurrency, 7);
        assert_eq!(settings.proxy, "http://127.0.0.1:8080");
        assert_eq!(settings.language, "en");
        drop(reopened);
        assert_eq!(config_entries(&database_path), stored_before_reopen);
    }

    #[test]
    fn migrates_legacy_download_schema() {
        let test_directory = TestDirectory::create("legacy");
        let database_path = test_directory.path().join(DATABASE_FILE_NAME);
        let connection = rusqlite::Connection::open(&database_path)
            .expect("legacy database file should be created");
        connection
            .execute_batch(
                "CREATE TABLE app_config (
                     key TEXT PRIMARY KEY,
                     value TEXT NOT NULL,
                     updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE downloads (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     url TEXT NOT NULL,
                     output_directory TEXT NOT NULL,
                     status TEXT NOT NULL,
                     error_message TEXT,
                     created_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL
                 );
                 CREATE TABLE download_logs (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     download_id INTEGER NOT NULL,
                     sequence INTEGER NOT NULL,
                     level TEXT NOT NULL,
                     message TEXT NOT NULL,
                     created_at INTEGER NOT NULL
                 );",
            )
            .expect("legacy schema should be created");
        drop(connection);

        let storage = Storage::open_or_initialize(&database_path)
            .expect("legacy schema should migrate successfully");
        let columns = download_columns(&storage);
        for column in [
            "resource_name",
            "format_selector",
            "video_format",
            "audio_format",
            "output_path",
            "downloaded_bytes",
            "total_bytes",
            "started_at",
            "completed_at",
        ] {
            assert!(columns.iter().any(|name| name == column));
        }
    }

    #[test]
    fn concurrent_initialization_serializes_schema_migration() {
        let test_directory = TestDirectory::create("concurrent");
        let database_path = test_directory.path().join(DATABASE_FILE_NAME);
        let barrier = Arc::new(Barrier::new(2));

        let handles = (0..2)
            .map(|_| {
                let database_path = database_path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    Storage::open_or_initialize(database_path)
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle
                .join()
                .expect("initialization thread should not panic")
                .expect("concurrent initialization should succeed");
        }

        let storage = Storage::open_or_initialize(&database_path)
            .expect("concurrently initialized database should reopen");
        assert_eq!(config_row_count(&storage), ALL_CONFIG_KEYS.len());
    }

    #[test]
    fn save_settings_rolls_back_when_one_key_fails() {
        let storage = Storage::open_or_initialize(":memory:")
            .expect("in-memory database should initialize defaults");
        let defaults = storage
            .load_settings()
            .expect("default settings should load");
        storage
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_proxy_update
                 BEFORE UPDATE ON app_config
                 WHEN NEW.key = 'proxy'
                 BEGIN
                     SELECT RAISE(ABORT, 'proxy update rejected');
                 END;",
            )
            .expect("failure trigger should be created");
        let changed = AppSettings {
            yt_dlp_path: "yt-dlp-custom.exe".into(),
            proxy: "http://127.0.0.1:7890".into(),
            max_concurrency: 8,
            language: "en".into(),
            ..defaults.clone()
        };

        storage
            .save_settings(&changed)
            .expect_err("one rejected key should fail the entire save");
        assert_eq!(
            storage
                .load_settings()
                .expect("settings should still be readable"),
            defaults
        );
    }

    #[test]
    fn open_failure_includes_stage_path_and_source() {
        let test_directory = TestDirectory::create("open-error");
        let database_path = test_directory.path().join(DATABASE_FILE_NAME);
        std::fs::create_dir(&database_path).expect("conflicting directory should be created");

        let error = match Storage::open_or_initialize(&database_path) {
            Ok(_) => panic!("a directory cannot be opened as a database file"),
            Err(error) => error,
        };
        match &error {
            AppError::StorageSqlite { stage, path, .. } => {
                assert_eq!(*stage, StorageStage::OpenDatabase);
                assert_eq!(path, &database_path);
            }
            other => panic!("expected a contextual SQLite open error, got {other:?}"),
        }
        assert!(
            error
                .to_string()
                .contains(&database_path.display().to_string())
        );
        assert!(error.source().is_some());
    }

    #[test]
    fn creates_missing_database_directory() {
        let test_directory = TestDirectory::create("nested");
        let database_path = test_directory
            .path()
            .join("missing-parent")
            .join(DATABASE_FILE_NAME);

        Storage::open_or_initialize(&database_path)
            .expect("missing database directory should be created");
        assert!(database_path.is_file());
    }

    #[test]
    fn derives_database_path_from_executable_directory() {
        let executable_path = Path::new("C:/Program Files/yt-dlp-gui/yt-dlp-gui.exe");
        let database_path = database_path_next_to_executable(executable_path)
            .expect("executable path should have a parent directory");
        assert_eq!(
            database_path,
            Path::new("C:/Program Files/yt-dlp-gui/application.sqlite3")
        );
    }

    fn config_row_count(storage: &Storage) -> usize {
        let count: i64 = storage
            .connection
            .query_row("SELECT COUNT(*) FROM app_config", [], |row| row.get(0))
            .expect("config rows should be countable");
        usize::try_from(count).expect("config row count should fit usize")
    }

    fn config_entry(storage: &Storage, key: &str) -> (String, i64) {
        storage
            .connection
            .query_row(
                "SELECT value, updated_at FROM app_config WHERE key = ?1",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("config entry should exist")
    }

    fn config_entries(database_path: &Path) -> Vec<(String, String, i64)> {
        let connection = rusqlite::Connection::open(database_path)
            .expect("database should open for config inspection");
        let mut statement = connection
            .prepare("SELECT key, value, updated_at FROM app_config ORDER BY key")
            .expect("config entries should be queryable");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("config entry query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("config entries should be readable")
    }

    fn object_names(storage: &Storage, object_type: &str) -> Vec<String> {
        let mut statement = storage
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = ?1")
            .expect("schema objects should be queryable");
        statement
            .query_map([object_type], |row| row.get(0))
            .expect("schema object query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("schema object names should be readable")
    }

    fn download_columns(storage: &Storage) -> Vec<String> {
        let mut statement = storage
            .connection
            .prepare("PRAGMA table_info(downloads)")
            .expect("download columns should be queryable");
        statement
            .query_map([], |row| row.get(1))
            .expect("download column query should run")
            .collect::<Result<Vec<_>, _>>()
            .expect("download column names should be readable")
    }
}
