use std::cmp::Ordering;
use std::path::Path;

use crate::download_task::{DownloadTaskError, MediaFormat, VideoInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchPathError {
    LeadingOrTrailingWhitespace,
    MissingDirectory,
    NotADirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFailure {
    ConfigurationMissing,
    YtDlpPathMissing,
    InvalidPath,
    Process,
    Metadata,
    TimedOut,
    Cancelled,
    Unexpected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    FormatId,
    FormatNote,
    Extension,
    Resolution,
    Bitrate,
    FileSize,
    VideoCodec,
    AudioCodec,
}

impl SortColumn {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::FormatId,
            1 => Self::FormatNote,
            2 => Self::Extension,
            3 => Self::Resolution,
            4 => Self::Bitrate,
            5 => Self::FileSize,
            6 => Self::VideoCodec,
            7 => Self::AudioCodec,
            _ => return None,
        })
    }

    pub const fn index(self) -> i32 {
        match self {
            Self::FormatId => 0,
            Self::FormatNote => 1,
            Self::Extension => 2,
            Self::Resolution => 3,
            Self::Bitrate => 4,
            Self::FileSize => 5,
            Self::VideoCodec => 6,
            Self::AudioCodec => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Reset,
    Ascending,
    Descending,
}

impl SortDirection {
    pub const fn index(self) -> i32 {
        match self {
            Self::Reset => 0,
            Self::Ascending => 1,
            Self::Descending => 2,
        }
    }
}

pub fn next_sort_state(
    current_column: Option<SortColumn>,
    current_direction: SortDirection,
    clicked: SortColumn,
) -> (Option<SortColumn>, SortDirection) {
    if current_column != Some(clicked) {
        return (Some(clicked), SortDirection::Ascending);
    }
    match current_direction {
        SortDirection::Reset => (Some(clicked), SortDirection::Ascending),
        SortDirection::Ascending => (Some(clicked), SortDirection::Descending),
        SortDirection::Descending => (None, SortDirection::Reset),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResultRow {
    pub format_id: String,
    pub format_note: String,
    pub extension: String,
    pub resolution: String,
    pub bitrate: String,
    pub file_size: String,
    pub video_codec: String,
    pub audio_codec: String,
}

pub fn validate_download_path(value: &str) -> Result<(), SearchPathError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.trim() != value {
        return Err(SearchPathError::LeadingOrTrailingWhitespace);
    }
    let path = Path::new(value);
    if !path.exists() {
        return Err(SearchPathError::MissingDirectory);
    }
    if !path.is_dir() {
        return Err(SearchPathError::NotADirectory);
    }
    Ok(())
}

pub fn can_download(path: &str, selected_index: Option<usize>, path_error: Option<SearchPathError>) -> bool {
    selected_index.is_some() && !path.is_empty() && path_error.is_none()
}

pub fn selected_download_streams(video: &VideoInfo, selected_index: Option<usize>) -> Option<(String, String)> {
    let video_format = selected_index
        .and_then(|index| video.formats.get(index))
        .filter(|format| is_video_only(format))?;
    let video_id = video_format.format_id.clone()?;
    let audio_id = video
        .formats
        .iter()
        .filter(|format| is_audio_only(format))
        .find(|format| format.extension.as_deref() == Some("m4a"))
        .or_else(|| video.formats.iter().find(|format| is_audio_only(format)))
        .and_then(|format| format.format_id.clone())?;
    Some((video_id, audio_id))
}

fn is_video_only(format: &MediaFormat) -> bool {
    format.format_id.is_some()
        && format.height.is_some()
        && format
            .video_codec
            .as_deref()
            .is_some_and(|codec| !codec.is_empty() && codec != "none")
        && format
            .audio_codec
            .as_deref()
            .is_none_or(|codec| codec.is_empty() || codec == "none")
}

fn is_audio_only(format: &MediaFormat) -> bool {
    format.format_id.is_some()
        && format
            .video_codec
            .as_deref()
            .is_none_or(|codec| codec.is_empty() || codec == "none")
        && format
            .audio_codec
            .as_deref()
            .is_some_and(|codec| !codec.is_empty() && codec != "none")
}

pub fn result_rows(video: &VideoInfo) -> Vec<SearchResultRow> {
    result_rows_in_order(video, &(0..video.formats.len()).collect::<Vec<_>>())
}

pub fn result_rows_in_order(video: &VideoInfo, order: &[usize]) -> Vec<SearchResultRow> {
    order
        .iter()
        .filter_map(|&index| video.formats.get(index).map(format_row))
        .collect()
}

pub fn sorted_result_indices(video: &VideoInfo, column: Option<SortColumn>, direction: SortDirection) -> Vec<usize> {
    let mut indices: Vec<_> = (0..video.formats.len()).collect();
    let Some(column) = column else { return indices };
    if direction == SortDirection::Reset {
        return indices;
    }
    indices.sort_by(|&left, &right| {
        let value_order = compare_formats(&video.formats[left], &video.formats[right], column);
        if value_order == Ordering::Equal {
            left.cmp(&right)
        } else if direction == SortDirection::Descending {
            reverse_present_order(value_order, &video.formats[left], &video.formats[right], column)
        } else {
            value_order
        }
    });
    indices
}

fn reverse_present_order(ordering: Ordering, left: &MediaFormat, right: &MediaFormat, column: SortColumn) -> Ordering {
    if is_missing(left, column) || is_missing(right, column) {
        ordering
    } else {
        ordering.reverse()
    }
}

fn compare_formats(left: &MediaFormat, right: &MediaFormat, column: SortColumn) -> Ordering {
    let left_missing = is_missing(left, column);
    let right_missing = is_missing(right, column);
    match (left_missing, right_missing) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => match column {
            SortColumn::FormatId => compare_text_or_number(left.format_id.as_deref(), right.format_id.as_deref()),
            SortColumn::FormatNote => compare_text(left.format_note.as_deref(), right.format_note.as_deref()),
            SortColumn::Extension => compare_text(left.extension.as_deref(), right.extension.as_deref()),
            SortColumn::Resolution => compare_resolution(left, right),
            SortColumn::Bitrate => compare_float(left.bitrate_kbps, right.bitrate_kbps),
            SortColumn::FileSize => left
                .filesize
                .or(left.filesize_approx)
                .cmp(&right.filesize.or(right.filesize_approx)),
            SortColumn::VideoCodec => compare_text(left.video_codec.as_deref(), right.video_codec.as_deref()),
            SortColumn::AudioCodec => compare_text(left.audio_codec.as_deref(), right.audio_codec.as_deref()),
        },
    }
}

fn is_missing(format: &MediaFormat, column: SortColumn) -> bool {
    match column {
        SortColumn::FormatId => format.format_id.is_none(),
        SortColumn::FormatNote => format.format_note.is_none(),
        SortColumn::Extension => format.extension.is_none(),
        SortColumn::Resolution => format.height.is_none() && format.width.is_none() && format.resolution.is_none(),
        SortColumn::Bitrate => format.bitrate_kbps.filter(|value| value.is_finite()).is_none(),
        SortColumn::FileSize => format.filesize.or(format.filesize_approx).is_none(),
        SortColumn::VideoCodec => format.video_codec.is_none(),
        SortColumn::AudioCodec => format.audio_codec.is_none(),
    }
}

fn compare_text(left: Option<&str>, right: Option<&str>) -> Ordering {
    left.unwrap_or_default()
        .to_ascii_lowercase()
        .cmp(&right.unwrap_or_default().to_ascii_lowercase())
}

fn compare_text_or_number(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (
        left.and_then(|value| value.parse::<u64>().ok()),
        right.and_then(|value| value.parse::<u64>().ok()),
    ) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => compare_text(left, right),
    }
}

fn compare_float(left: Option<f64>, right: Option<f64>) -> Ordering {
    left.zip(right)
        .and_then(|(left, right)| left.partial_cmp(&right))
        .unwrap_or(Ordering::Equal)
}

fn compare_resolution(left: &MediaFormat, right: &MediaFormat) -> Ordering {
    left.height
        .cmp(&right.height)
        .then_with(|| left.width.cmp(&right.width))
        .then_with(|| compare_text(left.resolution.as_deref(), right.resolution.as_deref()))
}

pub fn format_row(format: &MediaFormat) -> SearchResultRow {
    SearchResultRow {
        format_id: display_optional(&format.format_id),
        format_note: display_optional(&format.format_note),
        extension: display_optional(&format.extension),
        resolution: display_optional(&format.resolution),
        bitrate: format
            .bitrate_kbps
            .map(|value| format!("{value:.0} Kbps"))
            .unwrap_or_default(),
        file_size: format_size(format.filesize.or(format.filesize_approx)),
        video_codec: display_optional(&format.video_codec),
        audio_codec: display_optional(&format.audio_codec),
    }
}

pub fn classify_failure(error: &DownloadTaskError) -> SearchFailure {
    match error {
        DownloadTaskError::ExecutableNotFound(_) => SearchFailure::YtDlpPathMissing,
        DownloadTaskError::Timeout(_) => SearchFailure::TimedOut,
        DownloadTaskError::Cancelled => SearchFailure::Cancelled,
        DownloadTaskError::ProcessFailed { .. }
        | DownloadTaskError::Spawn(_)
        | DownloadTaskError::Io(_)
        | DownloadTaskError::VersionCommandFailed { .. }
        | DownloadTaskError::VersionOutputEmpty => SearchFailure::Process,
        DownloadTaskError::InvalidJson(_)
        | DownloadTaskError::MissingField(_)
        | DownloadTaskError::InvalidField { .. } => SearchFailure::Metadata,
        DownloadTaskError::InvalidDownloadRequest(_)
        | DownloadTaskError::ProgressParse(_)
        | DownloadTaskError::Storage(_)
        | DownloadTaskError::OutputPathMissing => SearchFailure::Unexpected,
        DownloadTaskError::DownloadProcessFailed { .. } => SearchFailure::Process,
        DownloadTaskError::Poisoned | DownloadTaskError::WorkerPanicked => SearchFailure::Unexpected,
    }
}

fn display_optional(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn format_size(value: Option<u64>) -> String {
    let Some(value) = value else { return String::new() };
    if value >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", value as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if value >= 1024 * 1024 {
        format!("{:.1} MiB", value as f64 / (1024.0 * 1024.0))
    } else if value >= 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else {
        format!("{value} B")
    }
}
