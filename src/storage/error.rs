use std::path::PathBuf;

use rusqlite::Error as SqlError;

/// 存储初始化和同步读写的失败原因，供启动层转换为用户可见错误提示。
#[derive(Debug)]
pub enum StorageError {
    /// 单例已经成功初始化，不能在同一进程内替换数据库。
    AlreadyInitialized,
    /// config 表不存在有效的环境配置记录。
    ConfigurationMissing,
    /// 无法取得当前可执行文件路径。
    ExecutablePath(std::io::Error),
    /// 可执行文件没有可用的父目录。
    InvalidDatabasePath(PathBuf),
    /// 其他模块在初始化前尝试访问单例。
    NotInitialized,
    /// SQLite 连接打开失败。
    Open(SqlError),
    /// 存储模块的互斥锁状态异常。
    Poisoned,
    /// SQLite 查询或结果映射失败。
    Read(SqlError),
    /// SQLite schema 初始化失败。
    Schema(SqlError),
    /// 配置数据库中的主题值不是 `system`、`light` 或 `dark`。
    InvalidTheme(String),
    /// 配置数据库中的语言不是当前支持的 locale。
    InvalidLanguage(String),
    /// 最大并发下载数不在支持范围内。
    InvalidConcurrentDownloads(i8),
    /// 数据库中的配置版本不是当前支持的版本。
    UnsupportedConfigurationVersion(String),
    /// SQLite 更新操作失败。
    Write(SqlError),
    /// 下载任务输入不满足持久化约束。
    InvalidDownloadInput,
    /// 下载任务进度不满足持久化约束。
    InvalidDownloadProgress,
    /// 下载任务标识不存在。
    DownloadNotFound(i64),
    /// 下载流标识不存在。
    DownloadStreamNotFound(i64),
    /// 下载流的任务不存在。
    DownloadStreamTaskNotFound(i64),
    /// 下载任务状态迁移不合法。
    InvalidDownloadStatusTransition,
    /// 数据库中保存了未知的下载任务状态。
    InvalidStoredDownloadStatus(String),
    /// 数据库中保存了未知的媒体类型。
    InvalidStoredMediaType(String),
    /// 当前数据库版本高于程序支持的版本。
    UnsupportedStorageSchemaVersion(i64),
    /// 下载流标识在同一任务中重复。
    DuplicateDownloadStream,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(formatter, "存储模块已经初始化。"),
            Self::ConfigurationMissing => write!(formatter, "配置数据库中不存在环境配置记录。"),
            Self::ExecutablePath(error) => write!(formatter, "无法定位应用程序文件：{error}"),
            Self::InvalidDatabasePath(path) => {
                write!(formatter, "应用程序目录无效：{}。", path.display())
            }
            Self::NotInitialized => write!(formatter, "存储模块尚未初始化。"),
            Self::Open(error) => write!(formatter, "无法打开配置数据库：{error}"),
            Self::Poisoned => write!(formatter, "存储模块状态异常。"),
            Self::Read(error) => write!(formatter, "无法读取环境配置：{error}"),
            Self::Schema(error) => write!(formatter, "无法初始化存储表结构：{error}"),
            Self::InvalidTheme(theme) => write!(formatter, "不支持的主题配置：{theme}。"),
            Self::InvalidLanguage(language) => write!(formatter, "不支持的界面语言：{language}。"),
            Self::InvalidConcurrentDownloads(value) => write!(formatter, "不支持的并发下载数：{value}。"),
            Self::UnsupportedConfigurationVersion(version) => write!(
                formatter,
                "不支持的配置版本：{version}，当前支持版本为 {}。",
                super::CONFIG_VERSION
            ),
            Self::Write(error) => write!(formatter, "无法保存环境配置：{error}"),
            Self::InvalidDownloadInput => write!(formatter, "下载任务输入无效。"),
            Self::InvalidDownloadProgress => write!(formatter, "下载任务进度无效。"),
            Self::DownloadNotFound(id) => write!(formatter, "下载任务不存在：{id}。"),
            Self::DownloadStreamNotFound(id) => write!(formatter, "下载流不存在：{id}。"),
            Self::DownloadStreamTaskNotFound(id) => write!(formatter, "下载流所属任务不存在：{id}。"),
            Self::InvalidDownloadStatusTransition => write!(formatter, "下载状态迁移无效。"),
            Self::InvalidStoredDownloadStatus(status) => write!(formatter, "数据库中的下载状态无效：{status}。"),
            Self::InvalidStoredMediaType(media_type) => write!(formatter, "数据库中的媒体类型无效：{media_type}。"),
            Self::UnsupportedStorageSchemaVersion(version) => {
                write!(formatter, "不支持的存储结构版本：{version}。")
            }
            Self::DuplicateDownloadStream => write!(formatter, "下载任务中的流标识重复。"),
        }
    }
}

impl std::error::Error for StorageError {}
