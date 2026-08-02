use super::config_keys::{ALL_CONFIG_KEYS, AppConfigKey};
use super::{Storage, timestamp};
use crate::app::state::AppSettings;
use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

const SELECT_VALUE_SQL: &str = "SELECT value FROM app_config WHERE key = ?1";
const INSERT_DEFAULT_SQL: &str =
    "INSERT INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)
     ON CONFLICT(key) DO NOTHING";
const UPSERT_VALUE_SQL: &str = "INSERT INTO app_config (key, value, updated_at) VALUES (?1, ?2, ?3)
     ON CONFLICT(key) DO UPDATE
     SET value = excluded.value, updated_at = excluded.updated_at";

/// Encapsulates SQLite access for application configuration values.
pub(super) struct AppConfigStore<'connection> {
    connection: &'connection Connection,
}

impl<'connection> AppConfigStore<'connection> {
    pub(super) const fn new(connection: &'connection Connection) -> Self {
        Self { connection }
    }

    pub(super) fn seed_defaults(
        &self,
        defaults: &AppSettings,
        updated_at: i64,
    ) -> rusqlite::Result<()> {
        let mut statement = self.connection.prepare_cached(INSERT_DEFAULT_SQL)?;
        for key in ALL_CONFIG_KEYS {
            statement.execute(params![key.as_str(), key.value_from(defaults), updated_at])?;
        }
        Ok(())
    }

    pub(super) fn get_value(&self, key: AppConfigKey) -> rusqlite::Result<Option<String>> {
        self.connection
            .query_row(SELECT_VALUE_SQL, [key.as_str()], |row| row.get(0))
            .optional()
    }

    pub(super) fn load_settings(&self) -> rusqlite::Result<AppSettings> {
        let mut settings = AppSettings::default();
        for key in ALL_CONFIG_KEYS {
            if let Some(value) = self.get_value(key)? {
                key.apply_to(&mut settings, value);
            }
        }
        Ok(settings)
    }

    pub(super) fn save_settings(
        &self,
        settings: &AppSettings,
        updated_at: i64,
    ) -> rusqlite::Result<()> {
        // Persist one complete snapshot so a failed key cannot leave mixed settings behind.
        let transaction = self.connection.unchecked_transaction()?;
        {
            let mut statement = transaction.prepare_cached(UPSERT_VALUE_SQL)?;
            for key in ALL_CONFIG_KEYS {
                statement.execute(params![key.as_str(), key.value_from(settings), updated_at])?;
            }
        }
        transaction.commit()
    }
}

impl Storage {
    pub fn load_settings(&self) -> Result<AppSettings, AppError> {
        Ok(AppConfigStore::new(&self.connection).load_settings()?)
    }

    /// Returns the configured path without checking the filesystem or executable behavior.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn require_yt_dlp_path(&self) -> Result<PathBuf, AppError> {
        let store = AppConfigStore::new(&self.connection);
        let value = store.get_value(AppConfigKey::YtDlpPath)?.ok_or_else(|| {
            AppError::NotFound("Required configuration 'yt_dlp_path' is missing".into())
        })?;
        let value = value.trim();
        if value.is_empty() {
            return Err(AppError::NotFound(
                "Required configuration 'yt_dlp_path' is empty".into(),
            ));
        }
        Ok(PathBuf::from(value))
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        AppConfigStore::new(&self.connection).save_settings(settings, timestamp())?;
        Ok(())
    }
}
