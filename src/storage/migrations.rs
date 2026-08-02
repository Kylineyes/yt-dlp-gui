use super::app_config::AppConfigStore;
use super::timestamp;
use crate::app::state::AppSettings;
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    // Keep the write lock until every schema and default-config change can commit together.
    connection.execute_batch("BEGIN IMMEDIATE;")?;
    let migration_result = migrate_schema(connection);
    match migration_result {
        Ok(()) => connection.execute_batch("COMMIT;"),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK;");
            Err(error)
        }
    }
}

fn migrate_schema(connection: &Connection) -> rusqlite::Result<()> {
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
        if !has_download_column(connection, column)? {
            connection.execute(
                &format!("ALTER TABLE downloads ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_download_logs_download_id
             ON download_logs(download_id, sequence);
         CREATE INDEX IF NOT EXISTS idx_downloads_updated_at
             ON downloads(updated_at DESC, id DESC);",
    )?;
    AppConfigStore::new(connection).seed_defaults(&AppSettings::default(), timestamp())?;
    connection.execute_batch("PRAGMA user_version = 1;")?;
    Ok(())
}

fn has_download_column(connection: &Connection, column: &str) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare("PRAGMA table_info(downloads)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for result in columns {
        if result? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
