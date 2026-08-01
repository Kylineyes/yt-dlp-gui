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

pub struct InspectRequest {
    pub url: String,
    pub yt_dlp_path: Option<PathBuf>,
    pub proxy: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InspectedMedia {
    pub resource_name: String,
    pub formats: Vec<InspectedFormat>,
}

#[derive(Debug, Clone)]
pub struct InspectedFormat {
    pub id: String,
    pub label: String,
    pub video_format: String,
    pub audio_format: String,
    pub estimated_size: String,
}

pub async fn inspect(request: InspectRequest) -> Result<Vec<InspectedMedia>, AppError> {
    let mut task = YtDlp::new(request.url).arg("--no-playlist");
    if let Some(path) = request.yt_dlp_path {
        task = task.yt_dlp_path(path.to_string_lossy().into_owned());
    }
    if let Some(proxy) = request.proxy {
        task = task.proxy(proxy);
    }

    let infos = task.get_info().await?;
    infos
        .into_iter()
        .map(|info| {
            let formats = info
                .extra
                .get("formats")
                .and_then(serde_json::Value::as_array)
                .map(|formats| formats.iter().filter_map(parse_format).collect())
                .unwrap_or_default();
            InspectedMedia {
                resource_name: info.title,
                formats,
            }
        })
        .collect::<Vec<_>>()
        .pipe(Ok)
}

fn parse_format(value: &serde_json::Value) -> Option<InspectedFormat> {
    let id = value.get("format_id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }

    let extension = string_value(value, "ext").unwrap_or_else(|| "unknown".into());
    let protocol = string_value(value, "protocol").unwrap_or_else(|| "unknown".into());
    let note = string_value(value, "format_note");
    let width = number_value(value, "width");
    let height = number_value(value, "height");
    let fps = number_value(value, "fps");
    let vcodec = codec_value(value, "vcodec");
    let acodec = codec_value(value, "acodec");
    let vbr = number_value(value, "vbr");
    let abr = number_value(value, "abr");
    let tbr = number_value(value, "tbr");

    if vcodec.is_none() && acodec.is_none() {
        return None;
    }

    let resolution = match (width, height) {
        (Some(width), Some(height)) => format!("{width}x{height}"),
        (None, Some(height)) => format!("{height}p"),
        _ => "unknown resolution".into(),
    };
    let video_format = vcodec.map_or_else(String::new, |codec| {
        let fps = fps.map_or_else(String::new, |value| format!(" · {value} fps"));
        let bitrate = vbr.map_or_else(String::new, |value| format!(" · {value} kbps"));
        format!("{resolution} · {codec}{fps}{bitrate}")
    });
    let audio_format = acodec.map_or_else(String::new, |codec| {
        let bitrate = abr.map_or_else(String::new, |value| format!(" · {value} kbps"));
        format!("{codec}{bitrate}")
    });
    let bitrate = tbr.map_or_else(String::new, |value| format!(" · {value} kbps"));
    let note = note.map_or_else(String::new, |value| format!(" · {value}"));
    let label = format!("{extension} · {protocol}{note}{bitrate}");
    let estimated_size = size_text(value);

    Some(InspectedFormat {
        id: id.into(),
        label,
        video_format,
        audio_format,
        estimated_size,
    })
}

fn string_value(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

fn codec_value(value: &serde_json::Value, key: &str) -> Option<String> {
    string_value(value, key).filter(|codec| codec != "none")
}

fn number_value(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|value| {
        value.as_f64().map(|number| {
            if number.fract() == 0.0 {
                format!("{number:.0}")
            } else {
                format!("{number:.2}")
            }
        })
    })
}

fn size_text(value: &serde_json::Value) -> String {
    if let Some(size) = value.get("filesize").and_then(serde_json::Value::as_f64) {
        return format_size(size, false);
    }
    if let Some(size) = value
        .get("filesize_approx")
        .and_then(serde_json::Value::as_f64)
    {
        return format_size(size, true);
    }
    "Unknown size".into()
}

fn format_size(size: f64, approximate: bool) -> String {
    let prefix = if approximate { "≈" } else { "" };
    let size = size.max(0.0);
    for (unit, scale) in [("GB", 1_073_741_824.0), ("MB", 1_048_576.0), ("KB", 1024.0)] {
        if size >= scale {
            return format!("{prefix}{:.1} {unit}", size / scale);
        }
    }
    format!("{prefix}{size:.0} B")
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

trait Pipe: Sized {
    fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T {
        function(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::parse_format;
    use serde_json::json;

    #[test]
    fn parses_video_audio_format_and_exact_size() {
        let format = parse_format(&json!({
            "format_id": "22",
            "ext": "mp4",
            "protocol": "https",
            "width": 1280,
            "height": 720,
            "fps": 30,
            "vcodec": "avc1.64001F",
            "acodec": "mp4a.40.2",
            "filesize": 2_097_152,
        }))
        .expect("format should be parsed");

        assert_eq!(format.id, "22");
        assert!(format.video_format.contains("1280x720"));
        assert!(format.audio_format.contains("mp4a.40.2"));
        assert_eq!(format.estimated_size, "2.0 MB");
    }

    #[test]
    fn parses_audio_only_and_approximate_size() {
        let format = parse_format(&json!({
            "format_id": "140",
            "ext": "m4a",
            "acodec": "mp4a.40.2",
            "vcodec": "none",
            "filesize_approx": 1024,
        }))
        .expect("format should be parsed");

        assert!(format.video_format.is_empty());
        assert_eq!(format.estimated_size, "≈1.0 KB");
    }

    #[test]
    fn skips_format_without_media_codec_or_id() {
        assert!(parse_format(&json!({"format_id": "", "vcodec": "avc1"})).is_none());
        assert!(
            parse_format(&json!({"format_id": "meta", "vcodec": "none", "acodec": "none"}))
                .is_none()
        );
    }
}
