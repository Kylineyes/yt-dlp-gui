mod config;
mod database;
mod error;
mod path;
pub mod schema;

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
    configuration: Mutex<Option<EnvironmentConfig>>,
}

impl Storage {
    /// 将 CLI 提供的可选数据库路径解析为最终存储文件路径。
    pub fn resolve_database_path(config_path: Option<PathBuf>) -> Result<PathBuf, StorageError> {
        path::resolve_database_path(config_path)
    }

    /// 打开或创建数据库，初始化全部 schema，并读取已有的环境配置。
    ///
    /// 首次创建的 config 表保持为空，具体配置由后续配置流程写入。
    pub fn initialize(database_path: PathBuf) -> Result<(), StorageError> {
        if STORAGE.get().is_some() {
            return Err(StorageError::AlreadyInitialized);
        }

        let connection = database::open_database(&database_path)?;
        schema::initialize_schema(&connection).map_err(StorageError::Schema)?;
        let configuration = database::read_configuration(&connection)?;
        if let Some(configuration) = &configuration {
            if configuration.version != CONFIG_VERSION {
                return Err(StorageError::UnsupportedConfigurationVersion(
                    configuration.version.clone(),
                ));
            }
            if !matches!(configuration.theme.as_str(), "system" | "light" | "dark") {
                return Err(StorageError::InvalidTheme(configuration.theme.clone()));
            }
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

    /// 同步读取当前内存中的环境配置；首次启动且 config 为空时返回 None。
    pub fn configuration(&self) -> Result<Option<EnvironmentConfig>, StorageError> {
        Ok(self.configuration.lock().map_err(|_| StorageError::Poisoned)?.clone())
    }

    pub fn version(&self) -> Result<Option<String>, StorageError> {
        Ok(self.configuration()?.map(|configuration| configuration.version))
    }

    pub fn yt_dlp_path(&self) -> Result<Option<String>, StorageError> {
        Ok(self.configuration()?.map(|configuration| configuration.yt_dlp_path))
    }

    pub fn ffmpeg_path(&self) -> Result<Option<String>, StorageError> {
        Ok(self.configuration()?.map(|configuration| configuration.ffmpeg_path))
    }

    pub fn default_download_path(&self) -> Result<Option<String>, StorageError> {
        Ok(self
            .configuration()?
            .map(|configuration| configuration.default_download_path))
    }

    pub fn theme(&self) -> Result<Option<String>, StorageError> {
        Ok(self.configuration()?.map(|configuration| configuration.theme))
    }

    pub fn language(&self) -> Result<Option<String>, StorageError> {
        Ok(self.configuration()?.map(|configuration| configuration.language))
    }

    pub fn concurrent_downloads(&self) -> Result<Option<i8>, StorageError> {
        Ok(self
            .configuration()?
            .map(|configuration| configuration.concurrent_downloads))
    }

    pub fn proxy(&self) -> Result<Option<String>, StorageError> {
        Ok(self.configuration()?.map(|configuration| configuration.proxy))
    }

    /// 原子保存完整环境配置；首次保存和后续更新都必须通过该接口完成。
    pub fn save_configuration(&self, configuration: EnvironmentConfig) -> Result<(), StorageError> {
        if configuration.version != CONFIG_VERSION {
            return Err(StorageError::UnsupportedConfigurationVersion(configuration.version));
        }
        if !matches!(configuration.theme.as_str(), "system" | "light" | "dark") {
            return Err(StorageError::InvalidTheme(configuration.theme));
        }
        {
            let mut connection = self.connection.lock().map_err(|_| StorageError::Poisoned)?;
            database::save_configuration(&mut connection, &configuration)?;
        }
        *self.configuration.lock().map_err(|_| StorageError::Poisoned)? = Some(configuration);
        Ok(())
    }

    /// 同步保存并更新 yt-dlp 的完整可执行文件路径。
    pub fn set_yt_dlp_path(&self, value: String) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.yt_dlp_path = value)
    }

    /// 同步保存并更新 FFmpeg 的完整可执行文件路径。
    pub fn set_ffmpeg_path(&self, value: String) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.ffmpeg_path = value)
    }

    /// 同步保存并更新默认下载完成目录。
    pub fn set_default_download_path(&self, value: String) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.default_download_path = value)
    }

    /// 同步保存并更新主题方案编号。
    pub fn set_theme(&self, value: String) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.theme = value)
    }

    /// 同步保存并更新界面语言标识。
    pub fn set_language(&self, value: String) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.language = value)
    }

    /// 同步保存并更新最大并发下载任务数量。
    pub fn set_concurrent_downloads(&self, value: i8) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.concurrent_downloads = value)
    }

    /// 同步保存并更新 yt-dlp 使用的代理地址。
    pub fn set_proxy(&self, value: String) -> Result<(), StorageError> {
        self.update_configuration(|configuration| configuration.proxy = value)
    }

    fn update_configuration(&self, update: impl FnOnce(&mut EnvironmentConfig)) -> Result<(), StorageError> {
        let mut configuration = self.configuration()?;
        let configuration = configuration.as_mut().ok_or(StorageError::ConfigurationMissing)?;
        update(configuration);
        self.save_configuration(configuration.clone())
    }
}
