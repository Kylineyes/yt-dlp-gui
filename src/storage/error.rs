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
    /// 数据库中的配置版本不是当前支持的版本。
    UnsupportedConfigurationVersion(String),
    /// SQLite 更新操作失败。
    Write(SqlError),
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
            Self::UnsupportedConfigurationVersion(version) => write!(
                formatter,
                "不支持的配置版本：{version}，当前支持版本为 {}。",
                super::CONFIG_VERSION
            ),
            Self::Write(error) => write!(formatter, "无法保存环境配置：{error}"),
        }
    }
}

impl std::error::Error for StorageError {}
