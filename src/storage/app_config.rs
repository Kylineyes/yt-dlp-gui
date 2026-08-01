use super::config_keys::ALL_CONFIG_KEYS;
use crate::app::state::AppSettings;
use rusqlite::{Connection, OptionalExtension, params};

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

    pub(super) fn load_settings(&self) -> rusqlite::Result<AppSettings> {
        let mut settings = AppSettings::default();
        let mut statement = self.connection.prepare_cached(SELECT_VALUE_SQL)?;
        for key in ALL_CONFIG_KEYS {
            let value = statement
                .query_row([key.as_str()], |row| row.get::<_, String>(0))
                .optional()?;
            if let Some(value) = value {
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
