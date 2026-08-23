use rusqlite::{Connection, Result};

/// 下载任务相关表结构的独立版本，不与配置版本混用。
pub const DOWNLOAD_SCHEMA_VERSION: i64 = 1;

/// 创建或升级全部存储表；schema 变更与版本记录在同一事务中提交。
pub fn initialize_schema(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS config (
            version TEXT NOT NULL,
            yt_dlp_path TEXT NOT NULL,
            ffmpeg_path TEXT NOT NULL,
            default_download_path TEXT NOT NULL,
            theme TEXT NOT NULL,
            language TEXT NOT NULL,
            concurrent_downloads INTEGER NOT NULL,
            proxy TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS storage_schema_versions (
            domain TEXT PRIMARY KEY,
            version INTEGER NOT NULL CHECK (version >= 1)
        );
        CREATE TABLE IF NOT EXISTS download_tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_url TEXT NOT NULL,
            video_id TEXT,
            title TEXT,
            thumbnail_url TEXT,
            duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds >= 0),
            target_path TEXT NOT NULL,
            output_path TEXT,
            selected_format TEXT,
            status TEXT NOT NULL CHECK (status IN ('pending', 'preparing', 'downloading', 'merging', 'completed', 'cancelled', 'failed')),
            progress_percent INTEGER CHECK (progress_percent IS NULL OR (progress_percent >= 0 AND progress_percent <= 100)),
            downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0),
            total_bytes INTEGER CHECK (total_bytes IS NULL OR total_bytes >= 0),
            total_bytes_estimate INTEGER CHECK (total_bytes_estimate IS NULL OR total_bytes_estimate >= 0),
            speed_bytes_per_second INTEGER CHECK (speed_bytes_per_second IS NULL OR speed_bytes_per_second >= 0),
            elapsed_seconds INTEGER CHECK (elapsed_seconds IS NULL OR elapsed_seconds >= 0),
            eta_seconds INTEGER CHECK (eta_seconds IS NULL OR eta_seconds >= 0),
            created_at INTEGER NOT NULL,
            started_at INTEGER,
            finished_at INTEGER,
            updated_at INTEGER NOT NULL,
            yt_dlp_version TEXT,
            error_code TEXT,
            error_message TEXT
        );
        CREATE TABLE IF NOT EXISTS download_task_streams (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL,
            stream_key TEXT NOT NULL,
            format_id TEXT,
            media_type TEXT NOT NULL CHECK (media_type IN ('video', 'audio')),
            extension TEXT,
            width INTEGER CHECK (width IS NULL OR width >= 0),
            height INTEGER CHECK (height IS NULL OR height >= 0),
            video_codec TEXT,
            audio_codec TEXT,
            status TEXT NOT NULL CHECK (status IN ('pending', 'preparing', 'downloading', 'merging', 'completed', 'cancelled', 'failed')),
            progress_percent INTEGER CHECK (progress_percent IS NULL OR (progress_percent >= 0 AND progress_percent <= 100)),
            downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (downloaded_bytes >= 0),
            total_bytes INTEGER CHECK (total_bytes IS NULL OR total_bytes >= 0),
            total_bytes_estimate INTEGER CHECK (total_bytes_estimate IS NULL OR total_bytes_estimate >= 0),
            speed_bytes_per_second INTEGER CHECK (speed_bytes_per_second IS NULL OR speed_bytes_per_second >= 0),
            elapsed_seconds INTEGER CHECK (elapsed_seconds IS NULL OR elapsed_seconds >= 0),
            eta_seconds INTEGER CHECK (eta_seconds IS NULL OR eta_seconds >= 0),
            created_at INTEGER NOT NULL,
            started_at INTEGER,
            finished_at INTEGER,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY (task_id) REFERENCES download_tasks(id) ON DELETE CASCADE,
            UNIQUE (task_id, stream_key)
        );
        CREATE INDEX IF NOT EXISTS idx_download_tasks_status_updated ON download_tasks(status, updated_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_download_tasks_created ON download_tasks(created_at DESC, id DESC);
        CREATE INDEX IF NOT EXISTS idx_download_task_streams_task ON download_task_streams(task_id);
        INSERT INTO storage_schema_versions (domain, version)
            VALUES ('download_tasks', 1)
            ON CONFLICT(domain) DO NOTHING;
        ",
    )?;
    let version: i64 = transaction.query_row(
        "SELECT version FROM storage_schema_versions WHERE domain = 'download_tasks'",
        [],
        |row| row.get(0),
    )?;
    if version > DOWNLOAD_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(
            "storage schema version".to_owned(),
        ));
    }
    transaction.commit()
}
