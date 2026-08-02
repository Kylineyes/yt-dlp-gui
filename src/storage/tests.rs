use super::config_keys::{ALL_CONFIG_KEYS, YT_DLP_PATH};
use super::{database_path_next_to_executable, Storage, DATABASE_FILE_NAME};
use crate::app::state::{AppSettings, NewDownload, ThemePreference};
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
fn persists_theme_preference_and_defaults_unknown_values() {
    let storage = Storage::open_or_initialize(":memory:")
        .expect("in-memory database should initialize defaults");
    assert_eq!(
        storage
            .load_settings()
            .expect("default settings should load")
            .theme_preference,
        ThemePreference::System
    );

    let settings = AppSettings {
        theme_preference: ThemePreference::Dark,
        ..AppSettings::default()
    };
    storage
        .save_settings(&settings)
        .expect("theme preference should be saved");
    assert_eq!(
        storage
            .load_settings()
            .expect("saved settings should load")
            .theme_preference,
        ThemePreference::Dark
    );

    set_config_value(
        &storage,
        super::config_keys::THEME_PREFERENCE,
        "unsupported",
    );
    assert_eq!(
        storage
            .load_settings()
            .expect("unknown theme preference should load")
            .theme_preference,
        ThemePreference::System
    );
}

#[test]
fn require_yt_dlp_path_rejects_missing_configuration() {
    let storage =
        Storage::open_or_initialize(":memory:").expect("in-memory database should initialize");
    storage
        .connection
        .execute("DELETE FROM app_config WHERE key = ?1", [YT_DLP_PATH])
        .expect("yt-dlp configuration should be removed");

    let error = storage
        .require_yt_dlp_path()
        .expect_err("missing yt-dlp path should fail");
    assert!(matches!(error, AppError::NotFound(_)));
    assert!(error.to_string().contains("missing"));
}

#[test]
fn require_yt_dlp_path_rejects_empty_configuration() {
    let storage =
        Storage::open_or_initialize(":memory:").expect("in-memory database should initialize");
    set_config_value(&storage, YT_DLP_PATH, "");

    let error = storage
        .require_yt_dlp_path()
        .expect_err("empty yt-dlp path should fail");
    assert!(matches!(error, AppError::NotFound(_)));
    assert!(error.to_string().contains("empty"));
}

#[test]
fn require_yt_dlp_path_rejects_whitespace_configuration() {
    let storage =
        Storage::open_or_initialize(":memory:").expect("in-memory database should initialize");
    set_config_value(&storage, YT_DLP_PATH, " \t\r\n ");

    let error = storage
        .require_yt_dlp_path()
        .expect_err("whitespace-only yt-dlp path should fail");
    assert!(matches!(error, AppError::NotFound(_)));
    assert!(error.to_string().contains("empty"));
}

#[test]
fn require_yt_dlp_path_returns_trimmed_path_without_validation() {
    let storage =
        Storage::open_or_initialize(":memory:").expect("in-memory database should initialize");
    set_config_value(&storage, YT_DLP_PATH, "  deliberately/missing/yt-dlp  ");

    let path = storage
        .require_yt_dlp_path()
        .expect("non-empty configured path should be returned");
    assert_eq!(path, PathBuf::from("deliberately/missing/yt-dlp"));
    assert!(!path.exists());
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
    let reopened =
        Storage::open_or_initialize(&database_path).expect("initialized database should reopen");
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
    let connection =
        rusqlite::Connection::open(&database_path).expect("legacy database file should be created");
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
    assert!(error
        .to_string()
        .contains(&database_path.display().to_string()));
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

fn set_config_value(storage: &Storage, key: &str, value: &str) {
    storage
        .connection
        .execute(
            "UPDATE app_config SET value = ?1 WHERE key = ?2",
            rusqlite::params![value, key],
        )
        .expect("config value should be updated");
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
