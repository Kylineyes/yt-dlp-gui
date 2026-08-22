fn main() -> Result<(), Box<dyn std::error::Error>> {
    // CLI 层统一解析参数，存储模块只负责把可选路径解析为最终数据库位置。
    let args = yt_dlp_gui::cli::AppArgs::parse()?;
    let database_path = yt_dlp_gui::storage::Storage::resolve_database_path(args.config_path)?;

    if let Err(error) = yt_dlp_gui::storage::Storage::initialize(database_path) {
        return yt_dlp_gui::app::window::show_storage_error(error);
    }

    yt_dlp_gui::app::window::run()?;
    Ok(())
}
