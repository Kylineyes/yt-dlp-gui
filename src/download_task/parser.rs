use serde_json::Value;

use super::error::DownloadTaskError;
use super::model::{
    DownloadMediaType, DownloadProgress, DownloadStage, DownloadStreamStatus, MediaFormat, StreamProgress, VideoInfo,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DownloadOutput {
    Progress(StreamProgress),
    Merging,
    OutputPath(String),
}

pub(crate) fn parse_download_line(
    line: &str,
    video_format_id: &str,
    audio_format_id: &str,
) -> Result<Option<DownloadOutput>, DownloadTaskError> {
    let line = line.trim();
    if line.starts_with("postprocess:") {
        return Ok(Some(DownloadOutput::Merging));
    }
    if let Some(path) = line.strip_prefix("after_move:") {
        return Ok(Some(DownloadOutput::OutputPath(path.to_owned())));
    }
    let fields = line
        .strip_prefix("download:")
        .ok_or_else(|| DownloadTaskError::ProgressParse(format!("未知进度行：{line}")))?
        .split('\t')
        .collect::<Vec<_>>();
    if fields.len() != 9 {
        return Err(DownloadTaskError::ProgressParse(format!(
            "字段数量为 {}，预期为 9",
            fields.len()
        )));
    }
    let format_id = value_string(fields[1]);
    let media_type = if format_id == video_format_id {
        DownloadMediaType::Video
    } else if format_id == audio_format_id {
        DownloadMediaType::Audio
    } else {
        return Err(DownloadTaskError::ProgressParse(format!("未知格式 ID：{format_id}")));
    };
    let status = match fields[0] {
        "downloading" => DownloadStreamStatus::Downloading,
        "finished" => DownloadStreamStatus::Finished,
        value => return Err(DownloadTaskError::ProgressParse(format!("未知状态：{value}"))),
    };
    let percent = parse_percent(fields[8])?;
    Ok(Some(DownloadOutput::Progress(StreamProgress {
        stream_key: format_id.clone(),
        format_id: Some(format_id),
        media_type,
        status,
        downloaded_bytes: parse_i64(fields[2])?.unwrap_or(0),
        total_bytes: parse_i64(fields[3])?,
        total_bytes_estimate: parse_i64(fields[4])?,
        speed_bytes_per_second: parse_speed(fields[5])?,
        elapsed_seconds: parse_seconds(fields[6])?,
        eta_seconds: parse_seconds(fields[7])?,
        percent,
        started_at: None,
        finished_at: None,
    })))
}

pub(crate) fn aggregate_progress(task_id: i64, streams: &[StreamProgress], updated_at: i64) -> DownloadProgress {
    let downloaded_bytes = streams.iter().map(|stream| stream.downloaded_bytes).sum();
    let total_bytes = streams.iter().map(|stream| stream.total_bytes).sum::<Option<i64>>();
    let fallback_total = streams
        .iter()
        .map(|stream| stream.total_bytes.or(stream.total_bytes_estimate))
        .sum::<Option<i64>>();
    let total_bytes_estimate = total_bytes.is_none().then_some(fallback_total).flatten();
    let total_is_estimate = total_bytes_estimate.is_some();
    let total = total_bytes.or(total_bytes_estimate);
    let percent = total.filter(|value| *value > 0).map(|value| {
        ((downloaded_bytes as f64 / value as f64) * 100.0)
            .clamp(0.0, 100.0)
            .round() as u8
    });
    let (speed_sum, speed_count) = streams
        .iter()
        .filter(|stream| stream.status == DownloadStreamStatus::Downloading)
        .filter_map(|stream| stream.speed_bytes_per_second)
        .fold((0, 0), |(sum, count), speed| (sum + speed, count + 1));
    let speed_bytes_per_second = (speed_count > 0).then_some(speed_sum);
    let elapsed_seconds = streams.iter().filter_map(|stream| stream.elapsed_seconds).max();
    let eta_seconds = total.filter(|value| *value > downloaded_bytes).and_then(|value| {
        speed_bytes_per_second
            .filter(|speed| *speed > 0)
            .map(|speed| (value - downloaded_bytes) / speed)
    });
    let active_stream = streams
        .iter()
        .find(|stream| stream.status == DownloadStreamStatus::Downloading)
        .map(|stream| stream.stream_key.clone());
    DownloadProgress {
        task_id,
        stage: DownloadStage::Downloading,
        downloaded_bytes,
        total_bytes,
        total_bytes_estimate,
        speed_bytes_per_second,
        elapsed_seconds,
        eta_seconds,
        percent,
        total_is_estimate,
        active_stream,
        updated_at,
    }
}
pub(crate) fn progress_template() -> String {
    "download:download:%(progress.status)s\t%(info.format_id)s\t%(progress.downloaded_bytes)s\t%(progress.total_bytes)s\t%(progress.total_bytes_estimate)s\t%(progress.speed)s\t%(progress.elapsed)s\t%(progress.eta)s\t%(progress._percent_str)s".to_owned()
}

pub(crate) fn postprocess_template() -> &'static str {
    "postprocess:postprocess:%(postprocessor_key)s"
}

fn value_string(value: &str) -> String {
    value.trim().to_owned()
}

fn parse_i64(value: &str) -> Result<Option<i64>, DownloadTaskError> {
    if matches!(value.trim(), "" | "NA" | "N/A" | "none" | "None") {
        return Ok(None);
    }
    value
        .trim()
        .parse::<i64>()
        .map(Some)
        .map_err(|_| DownloadTaskError::ProgressParse(format!("无效数值：{value}")))
}

fn parse_seconds(value: &str) -> Result<Option<i64>, DownloadTaskError> {
    let value = value.trim();
    if matches!(value, "" | "NA" | "N/A" | "none" | "None") {
        return Ok(None);
    }
    let seconds = value
        .parse::<f64>()
        .map_err(|_| DownloadTaskError::ProgressParse(format!("无效秒数：{value}")))?;
    if !seconds.is_finite() || seconds < 0.0 || seconds > i64::MAX as f64 {
        return Err(DownloadTaskError::ProgressParse(format!("秒数超出范围：{value}")));
    }
    Ok(Some(seconds.round() as i64))
}

fn parse_percent(value: &str) -> Result<Option<u8>, DownloadTaskError> {
    let value = value.trim().trim_end_matches('%').trim();
    if matches!(value, "" | "NA" | "N/A") {
        return Ok(None);
    }
    let percent = value
        .parse::<f64>()
        .map_err(|_| DownloadTaskError::ProgressParse(format!("无效百分比：{value}")))?;
    if !(0.0..=100.0).contains(&percent) {
        return Err(DownloadTaskError::ProgressParse(format!("百分比超出范围：{value}")));
    }
    Ok(Some(percent.round() as u8))
}

fn parse_speed(value: &str) -> Result<Option<i64>, DownloadTaskError> {
    let value = value.trim();
    if matches!(value, "" | "NA" | "N/A") {
        return Ok(None);
    }
    let mut parts = value.split_whitespace();
    let number = parts
        .next()
        .ok_or_else(|| DownloadTaskError::ProgressParse(format!("无效速度：{value}")))?
        .parse::<f64>()
        .map_err(|_| DownloadTaskError::ProgressParse(format!("无效速度：{value}")))?;
    let multiplier = match parts.next().unwrap_or("B/s") {
        "B/s" => 1.0,
        "KiB/s" => 1024.0,
        "MiB/s" => 1024.0 * 1024.0,
        "GiB/s" => 1024.0 * 1024.0 * 1024.0,
        unit => return Err(DownloadTaskError::ProgressParse(format!("未知速度单位：{unit}"))),
    };
    Ok(Some((number * multiplier) as i64))
}

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
