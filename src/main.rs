fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 命令行层只负责解析参数，数据库路径的默认值仍由存储模块决定。
    let args = yt_dlp_gui::cli::AppArgs::parse()?;
    let database_path = match args.config_path {
        Some(path) => path,
        None => yt_dlp_gui::storage::Storage::database_path_from_args()?,
    };

    if let Err(error) = yt_dlp_gui::storage::Storage::initialize(database_path) {
        return yt_dlp_gui::app::window::show_storage_error(error);
    }

    yt_dlp_gui::app::window::run()?;
    Ok(())
}
