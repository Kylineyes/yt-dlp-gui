fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_path = match yt_dlp_gui::storage::Storage::database_path_from_args() {
        Ok(path) => path,
        Err(error) => return yt_dlp_gui::app::window::show_storage_error(error),
    };
    if let Err(error) = yt_dlp_gui::storage::Storage::initialize(database_path) {
        return yt_dlp_gui::app::window::show_storage_error(error);
    }

    yt_dlp_gui::app::window::run()?;
    Ok(())
}
