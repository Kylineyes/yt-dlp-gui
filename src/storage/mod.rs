mod config;
mod database;
mod error;
mod path;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;

pub use config::EnvironmentConfig;
pub use error::StorageError;

/// 当前程序能够直接读取的配置格式版本。
pub const CONFIG_VERSION: &str = "0.0.1";

/// 进程级单例；OnceLock 保证初始化只成功一次且实例地址保持稳定。
static STORAGE: OnceLock<Storage> = OnceLock::new();

/// 存储模块的同步访问入口，独占管理 SQLite 连接、路径和已加载的环境配置。
pub struct Storage {
    /// 启动时解析并锁定的数据库路径，运行期间不允许切换。
    database_path: PathBuf,
    /// SQLite 连接由互斥锁保护，所有数据库操作都经过 Storage。
    connection: Mutex<Connection>,
    /// 与数据库唯一配置记录对应的同步内存快照。
    configuration: Mutex<EnvironmentConfig>,
}

impl Storage {
    /// 将 CLI 提供的可选数据库路径解析为最终存储文件路径。
    pub fn resolve_database_path(config_path: Option<PathBuf>) -> Result<PathBuf, StorageError> {
        path::resolve_database_path(config_path)
    }

    /// 打开既有数据库并读取唯一的环境配置记录；该函数不会创建文件或表。
    pub fn initialize(database_path: PathBuf) -> Result<(), StorageError> {
        if STORAGE.get().is_some() {
            return Err(StorageError::AlreadyInitialized);
        }

        let connection = database::open_existing_database(&database_path)?;
        let configuration = database::read_configuration(&connection)?;
        if configuration.version != CONFIG_VERSION {
            return Err(StorageError::UnsupportedConfigurationVersion(configuration.version));
        }
        STORAGE
            .set(Self {
                database_path,
                connection: Mutex::new(connection),
                configuration: Mutex::new(configuration),
            })
            .map_err(|_| StorageError::AlreadyInitialized)
    }

    /// 返回进程内唯一的存储实例；应用启动完成前调用会失败。
    pub fn instance() -> Result<&'static Self, StorageError> {
        STORAGE.get().ok_or(StorageError::NotInitialized)
    }

    /// 返回启动时确认可读取的数据库路径。
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// 同步读取当前内存中的完整环境配置快照。
    pub fn configuration(&self) -> Result<EnvironmentConfig, StorageError> {
        Ok(self.configuration.lock().map_err(|_| StorageError::Poisoned)?.clone())
    }

    pub fn version(&self) -> Result<String, StorageError> {
        Ok(self.configuration()?.version)
    }

    pub fn yt_dlp_path(&self) -> Result<String, StorageError> {
        Ok(self.configuration()?.yt_dlp_path)
    }

    pub fn ffmpeg_path(&self) -> Result<String, StorageError> {
        Ok(self.configuration()?.ffmpeg_path)
    }

    pub fn default_download_path(&self) -> Result<String, StorageError> {
        Ok(self.configuration()?.default_download_path)
    }

    pub fn theme(&self) -> Result<i8, StorageError> {
        Ok(self.configuration()?.theme)
    }

    pub fn language(&self) -> Result<String, StorageError> {
        Ok(self.configuration()?.language)
    }

    pub fn concurrent_downloads(&self) -> Result<i8, StorageError> {
        Ok(self.configuration()?.concurrent_downloads)
    }

    pub fn proxy(&self) -> Result<String, StorageError> {
        Ok(self.configuration()?.proxy)
    }

    /// 同步保存并更新 yt-dlp 的完整可执行文件路径。
    pub fn set_yt_dlp_path(&self, value: String) -> Result<(), StorageError> {
        self.update_text("yt_dlp_path", value, |config, value| config.yt_dlp_path = value)
    }

    /// 同步保存并更新 FFmpeg 的完整可执行文件路径。
    pub fn set_ffmpeg_path(&self, value: String) -> Result<(), StorageError> {
        self.update_text("ffmpeg_path", value, |config, value| config.ffmpeg_path = value)
    }

    /// 同步保存并更新默认下载完成目录。
    pub fn set_default_download_path(&self, value: String) -> Result<(), StorageError> {
        self.update_text("default_download_path", value, |config, value| {
            config.default_download_path = value
        })
    }

    /// 同步保存并更新主题方案编号。
    pub fn set_theme(&self, value: i8) -> Result<(), StorageError> {
        self.update_integer("theme", value, |config, value| config.theme = value)
    }

    /// 同步保存并更新界面语言标识。
    pub fn set_language(&self, value: String) -> Result<(), StorageError> {
        self.update_text("language", value, |config, value| config.language = value)
    }

    /// 同步保存并更新最大并发下载任务数量。
    pub fn set_concurrent_downloads(&self, value: i8) -> Result<(), StorageError> {
        self.update_integer("concurrent_downloads", value, |config, value| {
            config.concurrent_downloads = value
        })
    }

    /// 同步保存并更新 yt-dlp 使用的代理地址。
    pub fn set_proxy(&self, value: String) -> Result<(), StorageError> {
        self.update_text("proxy", value, |config, value| config.proxy = value)
    }

    fn update_text(
        &self,
        field: &str,
        value: String,
        update: impl FnOnce(&mut EnvironmentConfig, String),
    ) -> Result<(), StorageError> {
        {
            let connection = self.connection.lock().map_err(|_| StorageError::Poisoned)?;
            database::update_text(&connection, field, &value)?;
        }
        update(
            &mut *self.configuration.lock().map_err(|_| StorageError::Poisoned)?,
            value,
        );
        Ok(())
    }

    fn update_integer(
        &self,
        field: &str,
        value: i8,
        update: impl FnOnce(&mut EnvironmentConfig, i8),
    ) -> Result<(), StorageError> {
        {
            let connection = self.connection.lock().map_err(|_| StorageError::Poisoned)?;
            database::update_integer(&connection, field, value)?;
        }
        update(
            &mut *self.configuration.lock().map_err(|_| StorageError::Poisoned)?,
            value,
        );
        Ok(())
    }
}
