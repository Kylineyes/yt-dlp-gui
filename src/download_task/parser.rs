use serde_json::Value;

use super::error::DownloadTaskError;
use super::model::{MediaFormat, VideoInfo};

/// 只提取页面展示和后续格式选择所需字段，未知 yt-dlp 字段会被保留在输入之外而忽略。
pub(crate) fn parse_video_info(output: &[u8]) -> Result<VideoInfo, DownloadTaskError> {
    let value: Value =
        serde_json::from_slice(output).map_err(|error| DownloadTaskError::InvalidJson(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| DownloadTaskError::InvalidJson("顶层 JSON 必须是对象".to_owned()))?;

    Ok(VideoInfo {
        id: required_string(object, "id")?,
        title: required_string(object, "title")?,
        webpage_url: optional_string(object, "webpage_url")?,
        original_url: optional_string(object, "original_url")?,
        uploader: optional_string(object, "uploader")?,
        channel: optional_string(object, "channel")?,
        duration_seconds: optional_number(object, "duration")?,
        thumbnail_url: optional_string(object, "thumbnail")?,
        description: optional_string(object, "description")?,
        upload_date: optional_string(object, "upload_date")?,
        // 某些站点可能不提供 formats，空列表比让整个视频元数据失败更适合展示层。
        formats: object
            .get("formats")
            .map(parse_formats)
            .transpose()?
            .unwrap_or_default(),
    })
}

fn parse_formats(value: &Value) -> Result<Vec<MediaFormat>, DownloadTaskError> {
    let formats = value.as_array().ok_or_else(|| invalid_field("formats", "必须是数组"))?;
    formats.iter().map(parse_format).collect()
}

fn parse_format(value: &Value) -> Result<MediaFormat, DownloadTaskError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_field("formats", "数组元素必须是对象"))?;
    Ok(MediaFormat {
        format_id: optional_string(object, "format_id")?,
        format_note: optional_string(object, "format_note")?,
        extension: optional_string(object, "ext")?,
        resolution: optional_string(object, "resolution")?,
        width: optional_integer(object, "width")?,
        height: optional_integer(object, "height")?,
        fps: optional_number(object, "fps")?,
        filesize: optional_integer(object, "filesize")?,
        filesize_approx: optional_integer(object, "filesize_approx")?,
        bitrate_kbps: optional_number(object, "tbr")?,
        video_codec: optional_string(object, "vcodec")?,
        audio_codec: optional_string(object, "acodec")?,
        audio_bitrate_kbps: optional_number(object, "abr")?,
        video_bitrate_kbps: optional_number(object, "vbr")?,
        protocol: optional_string(object, "protocol")?,
        url: optional_string(object, "url")?,
    })
}

fn required_string(object: &serde_json::Map<String, Value>, field: &'static str) -> Result<String, DownloadTaskError> {
    match object.get(field) {
        Some(Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(_) => Err(invalid_field(field, "必须是非空字符串")),
        None => Err(DownloadTaskError::MissingField(field)),
    }
}

fn optional_string(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<String>, DownloadTaskError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(invalid_field(field, "必须是字符串或 null")),
    }
}

fn optional_number(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<f64>, DownloadTaskError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| invalid_field(field, "必须是数字或 null")),
    }
}

fn optional_integer(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<u64>, DownloadTaskError> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| invalid_field(field, "必须是非负整数或 null")),
    }
}

fn invalid_field(field: &'static str, message: &str) -> DownloadTaskError {
    DownloadTaskError::InvalidField {
        field,
        message: message.to_owned(),
    }
}
