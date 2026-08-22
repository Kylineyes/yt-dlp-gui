use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

/// 应用启动参数；未指定配置路径时由存储模块解析默认位置。
#[derive(Debug, PartialEq, Eq)]
pub struct AppArgs {
    /// 通过 `-c <路径>` 指定的配置数据库路径。
    pub config_path: Option<PathBuf>,
}

impl AppArgs {
    /// 解析当前进程收到的命令行参数。
    pub fn parse() -> Result<Self, CliError> {
        Self::parse_from(env::args_os().skip(1))
    }

    /// 从参数序列解析启动参数，便于启动流程和独立测试复用同一规则。
    pub fn parse_from<I>(arguments: I) -> Result<Self, CliError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut config_path = None;
        let mut arguments = arguments.into_iter();

        while let Some(argument) = arguments.next() {
            if argument == "-c" {
                if config_path.is_some() {
                    return Err(CliError::DuplicateConfigPath);
                }

                let path = arguments.next().ok_or(CliError::MissingConfigPath)?;
                config_path = Some(PathBuf::from(path));
            } else {
                return Err(CliError::UnexpectedArgument(argument));
            }
        }

        Ok(Self { config_path })
    }
}

/// 命令行参数解析失败的原因。
#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    /// 重复指定配置数据库路径。
    DuplicateConfigPath,
    /// `-c` 参数没有对应的路径值。
    MissingConfigPath,
    /// 参数不属于当前支持的命令行选项。
    UnexpectedArgument(OsString),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateConfigPath => write!(formatter, "参数 -c 只能指定一次。"),
            Self::MissingConfigPath => write!(formatter, "参数 -c 后缺少配置数据库路径。"),
            Self::UnexpectedArgument(argument) => {
                write!(formatter, "不支持的命令行参数：{}。", argument.to_string_lossy())
            }
        }
    }
}

impl std::error::Error for CliError {}
