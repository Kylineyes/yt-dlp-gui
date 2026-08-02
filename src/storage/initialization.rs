use super::{Storage, database_path_next_to_executable, migrations, schema};
use crate::error::{AppError, StorageStage};
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

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
        configure_connection(&connection).map_err(|source| AppError::StorageSqlite {
            stage: StorageStage::ConfigureConnection,
            path: database_path.clone(),
            source,
        })?;
        schema::create_tables(&connection).map_err(|source| AppError::StorageSqlite {
            stage: StorageStage::CreateTables,
            path: database_path.clone(),
            source,
        })?;

        // Expose the handle only after every connection and schema invariant is established.
        let storage = Self { connection };
        migrations::migrate(&storage.connection).map_err(|source| AppError::StorageSqlite {
            stage: StorageStage::MigrateSchema,
            path: database_path,
            source,
        })?;
        Ok(storage)
    }
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
                // WAL setup briefly takes an exclusive lock when processes initialize together.
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("connection configuration attempts should return")
}
