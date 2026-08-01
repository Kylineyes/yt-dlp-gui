use rusqlite::Connection;

/// Creates the baseline tables required before schema migrations run.
pub(super) fn create_tables(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_config (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS downloads (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             url TEXT NOT NULL,
             resource_name TEXT NOT NULL DEFAULT '',
             output_directory TEXT NOT NULL,
             output_path TEXT NOT NULL DEFAULT '',
             status TEXT NOT NULL,
             format_selector TEXT NOT NULL DEFAULT '',
             video_format TEXT NOT NULL DEFAULT '',
             audio_format TEXT NOT NULL DEFAULT '',
             downloaded_bytes INTEGER NOT NULL DEFAULT 0,
             total_bytes INTEGER NOT NULL DEFAULT 0,
             started_at INTEGER,
             completed_at INTEGER,
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
    )
}
