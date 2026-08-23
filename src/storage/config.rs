use rusqlite::{Error as SqlError, Row};

/// 应用环境配置的内存快照，所有字段与 config 表的唯一记录一一对应。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentConfig {
    /// 配置格式版本；启动时必须与当前程序支持的版本一致。
    pub version: String,
    /// yt-dlp 可执行文件的完整路径。
    pub yt_dlp_path: String,
    /// FFmpeg 可执行文件的完整路径。
    pub ffmpeg_path: String,
    /// 下载完成后默认使用的目录，空字符串表示尚未设置。
    pub default_download_path: String,
    /// 持久化的主题方案：`system`、`light` 或 `dark`。
    pub theme: String,
    /// 持久化的 BCP-47 语言标识，例如 `zh-CN` 或 `en-US`。
    pub language: String,
    /// 同时运行的最大下载任务数量。
    pub concurrent_downloads: i8,
    /// yt-dlp 请求使用的代理地址，空字符串表示不使用代理。
    pub proxy: String,
}

impl EnvironmentConfig {
    /// 配置页面首次打开时使用的草稿默认值。
    pub fn draft_default() -> Self {
        Self {
            version: super::CONFIG_VERSION.to_string(),
            yt_dlp_path: String::new(),
            ffmpeg_path: String::new(),
            default_download_path: String::new(),
            theme: "system".to_string(),
            language: "en-US".to_string(),
            concurrent_downloads: 0,
            proxy: String::new(),
        }
    }

    /// 按固定列顺序读取 config 表，列顺序必须与数据库查询保持一致。
    pub(super) fn from_row(row: &Row<'_>) -> Result<Self, SqlError> {
        Ok(Self {
            version: row.get(0)?,
            yt_dlp_path: row.get(1)?,
            ffmpeg_path: row.get(2)?,
            default_download_path: row.get(3)?,
            theme: row.get(4)?,
            language: row.get(5)?,
            concurrent_downloads: row.get(6)?,
            proxy: row.get(7)?,
        })
    }
}
