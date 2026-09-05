use std::cmp::Ordering;
use std::path::Path;

use crate::design_system::i18n::{I18nCatalog, Locale, TextKey};
use crate::download_task::DownloadRequest;
use crate::storage::{DownloadTask, DownloadTaskStatus};
use crate::table::{compare_lexicographic, TableRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSortColumn {
    Title,
    Status,
    Progress,
    Size,
    Speed,
    Eta,
    UpdatedAt,
    TargetPath,
}

impl TaskSortColumn {
    pub const ALL: [Self; 8] = [
        Self::Title,
        Self::Status,
        Self::Progress,
        Self::Size,
        Self::Speed,
        Self::Eta,
        Self::UpdatedAt,
        Self::TargetPath,
    ];

    pub const fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Title),
            1 => Some(Self::Status),
            2 => Some(Self::Progress),
            3 => Some(Self::Size),
            4 => Some(Self::Speed),
            5 => Some(Self::Eta),
            6 => Some(Self::UpdatedAt),
            7 => Some(Self::TargetPath),
            _ => None,
        }
    }

    pub const fn index(self) -> i32 {
        match self {
            Self::Title => 0,
            Self::Status => 1,
            Self::Progress => 2,
            Self::Size => 3,
            Self::Speed => 4,
            Self::Eta => 5,
            Self::UpdatedAt => 6,
            Self::TargetPath => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSortDirection {
    Reset,
    Ascending,
    Descending,
}

impl TaskSortDirection {
    pub const fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Reset),
            1 => Some(Self::Ascending),
            2 => Some(Self::Descending),
            _ => None,
        }
    }

    pub const fn index(self) -> i32 {
        match self {
            Self::Reset => 0,
            Self::Ascending => 1,
            Self::Descending => 2,
        }
    }

    fn apply(self, ordering: Ordering) -> Ordering {
        if self == Self::Descending {
            ordering.reverse()
        } else {
            ordering
        }
    }
}

pub fn task_row(task: &DownloadTask, locale: Locale) -> TableRow {
    TableRow::new(vec![
        task_title(task).to_owned(),
        task_status_text(locale, task.status).to_owned(),
        format_progress(task.progress_percent),
        format_task_size(task),
        format_speed(task.speed_bytes_per_second),
        format_eta(task.eta_seconds),
        format_timestamp(task.updated_at),
        task.target_path.clone(),
    ])
}

pub fn task_title(task: &DownloadTask) -> &str {
    task.title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&task.source_url)
}

pub fn format_yt_dlp_download_command(
    request: &DownloadRequest,
    ffmpeg_path: Option<&Path>,
    proxy: Option<&str>,
) -> String {
    let mut arguments = vec![
        "-f".to_owned(),
        format!(
            "{}+{}",
            request.selected_video_format_id, request.selected_audio_format_id
        ),
        "--merge-output-format".to_owned(),
        request.merge_output_format.clone(),
        "--no-overwrites".to_owned(),
        "-P".to_owned(),
        format!("home:{}", request.target_directory.display()),
        "-P".to_owned(),
        format!("temp:{}", request.temporary_directory.display()),
        "-o".to_owned(),
        request.output_template.clone(),
    ];
    if let Some(proxy) = proxy.filter(|value| !value.trim().is_empty()) {
        arguments.extend(["--proxy".to_owned(), proxy.to_owned()]);
    }
    if let Some(ffmpeg_path) = ffmpeg_path.filter(|path| !path.as_os_str().is_empty()) {
        arguments.extend(["--ffmpeg-location".to_owned(), ffmpeg_path.display().to_string()]);
    }
    if let Some(rate_limit) = &request.options.rate_limit {
        arguments.extend(["--limit-rate".to_owned(), rate_limit.clone()]);
    }
    for (name, value) in [
        ("--retries", request.options.retries),
        ("--fragment-retries", request.options.fragment_retries),
        ("--file-access-retries", request.options.file_access_retries),
        ("--concurrent-fragments", request.options.concurrent_fragments),
    ] {
        if let Some(value) = value {
            arguments.extend([name.to_owned(), value.to_string()]);
        }
    }
    arguments.push(request.source_url.clone());

    std::iter::once("yt-dlp".to_owned())
        .chain(arguments.into_iter().map(|argument| powershell_argument(&argument)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn powershell_argument(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub const fn task_status_key(status: DownloadTaskStatus) -> TextKey {
    match status {
        DownloadTaskStatus::Pending => TextKey::TasksStatusPending,
        DownloadTaskStatus::Preparing => TextKey::TasksStatusPreparing,
        DownloadTaskStatus::Downloading => TextKey::TasksStatusDownloading,
        DownloadTaskStatus::Paused => TextKey::TasksStatusPaused,
        DownloadTaskStatus::Merging => TextKey::TasksStatusMerging,
        DownloadTaskStatus::Completed => TextKey::TasksStatusCompleted,
        DownloadTaskStatus::Cancelled => TextKey::TasksStatusCancelled,
        DownloadTaskStatus::Failed => TextKey::TasksStatusFailed,
    }
}

pub fn task_status_text(locale: Locale, status: DownloadTaskStatus) -> &'static str {
    I18nCatalog::text(locale, task_status_key(status))
}

pub fn has_active_tasks(tasks: &[DownloadTask]) -> bool {
    tasks
        .iter()
        .any(|task| !task.status.is_terminal() && task.status != DownloadTaskStatus::Paused)
}

pub fn format_progress(progress_percent: Option<u8>) -> String {
    progress_percent.map_or_else(String::new, |progress| format!("{progress}%"))
}

pub fn format_task_size(task: &DownloadTask) -> String {
    format_bytes(task.downloaded_bytes)
}

pub fn format_speed(speed_bytes_per_second: Option<i64>) -> String {
    speed_bytes_per_second
        .filter(|&speed| speed >= 0)
        .map_or_else(String::new, |speed| format!("{}/s", format_bytes(speed)))
}

pub fn format_eta(eta_seconds: Option<i64>) -> String {
    eta_seconds.map_or_else(String::new, format_duration)
}

pub fn format_timestamp(timestamp: i64) -> String {
    if timestamp < 0 {
        return String::new();
    }

    let days = timestamp.div_euclid(86_400);
    let seconds_in_day = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_in_day / 3_600;
    let minute = seconds_in_day % 3_600 / 60;
    let second = seconds_in_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

pub fn sorted_task_indices(
    tasks: &[DownloadTask],
    column: Option<TaskSortColumn>,
    direction: TaskSortDirection,
) -> Vec<usize> {
    let mut indices = (0..tasks.len()).collect::<Vec<_>>();
    let Some(column) = column.filter(|_| direction != TaskSortDirection::Reset) else {
        return indices;
    };

    indices.sort_by(|&left_index, &right_index| {
        let left = &tasks[left_index];
        let right = &tasks[right_index];
        compare_tasks(left, right, column, direction)
            .then_with(|| left.id.cmp(&right.id))
            .then_with(|| left_index.cmp(&right_index))
    });
    indices
}

fn compare_tasks(
    left: &DownloadTask,
    right: &DownloadTask,
    column: TaskSortColumn,
    direction: TaskSortDirection,
) -> Ordering {
    match column {
        TaskSortColumn::Title => direction.apply(compare_lexicographic(task_title(left), task_title(right))),
        TaskSortColumn::Status => direction.apply(status_rank(left.status).cmp(&status_rank(right.status))),
        TaskSortColumn::Progress => {
            compare_optional(left.progress_percent, right.progress_percent, direction, Ord::cmp)
        }
        TaskSortColumn::Size => direction.apply(task_size_value(left).cmp(&task_size_value(right))),
        TaskSortColumn::Speed => compare_optional(
            left.speed_bytes_per_second,
            right.speed_bytes_per_second,
            direction,
            Ord::cmp,
        ),
        TaskSortColumn::Eta => compare_optional(left.eta_seconds, right.eta_seconds, direction, Ord::cmp),
        TaskSortColumn::UpdatedAt => direction.apply(left.updated_at.cmp(&right.updated_at)),
        TaskSortColumn::TargetPath => direction.apply(compare_lexicographic(&left.target_path, &right.target_path)),
    }
}

fn compare_optional<T>(
    left: Option<T>,
    right: Option<T>,
    direction: TaskSortDirection,
    compare: impl FnOnce(&T, &T) -> Ordering,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => direction.apply(compare(&left, &right)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

const fn status_rank(status: DownloadTaskStatus) -> u8 {
    match status {
        DownloadTaskStatus::Pending => 0,
        DownloadTaskStatus::Preparing => 1,
        DownloadTaskStatus::Downloading => 2,
        DownloadTaskStatus::Paused => 3,
        DownloadTaskStatus::Merging => 4,
        DownloadTaskStatus::Completed => 5,
        DownloadTaskStatus::Cancelled => 6,
        DownloadTaskStatus::Failed => 7,
    }
}

fn task_size_value(task: &DownloadTask) -> i64 {
    task.downloaded_bytes
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 0 {
        return String::new();
    }

    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_duration(seconds: i64) -> String {
    if seconds < 0 {
        return String::new();
    }

    let hours = seconds / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

// 将 Unix epoch 天数转换为公历日期，不引入额外日期依赖。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}
