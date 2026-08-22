use rusqlite::{Connection, Result};

/// 所有存储表的创建入口；后续新增表时在此模块扩展初始化步骤。
///
/// schema 只定义结构，不插入配置数据，保证首次启动不会替用户做配置选择。
pub fn initialize_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "\
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
        ",
    )
}
