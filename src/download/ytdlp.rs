use crate::error::AppError;
use std::path::PathBuf;
use ytd_rs::YtDlp;

pub struct DownloadRequest {
    pub url: String,
    pub output_directory: PathBuf,
    pub yt_dlp_path: Option<PathBuf>,
    pub ffmpeg_path: Option<PathBuf>,
    pub proxy: Option<String>,
    pub format_selector: String,
}

pub async fn download<F>(request: DownloadRequest, mut on_line: F) -> Result<(), AppError>
where
    F: FnMut(String),
{
    let mut task = YtDlp::new(request.url)
        .output_dir(request.output_directory)
        .arg("--newline")
        .format(request.format_selector);

    if let Some(path) = request.yt_dlp_path {
        task = task.yt_dlp_path(path.to_string_lossy().into_owned());
    }
    if let Some(path) = request.ffmpeg_path {
        task = task.arg_with("--ffmpeg-location", path.to_string_lossy().into_owned());
    }
    if let Some(proxy) = request.proxy {
        task = task.proxy(proxy);
    }

    let mut process = task.download_process().await?;
    while let Some(line) = process.next_line().await? {
        on_line(line);
    }
    process.wait().await?;
    Ok(())
}
