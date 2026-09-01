use std::fs;

use yt_dlp_gui::app::tasks::{
    format_eta, format_progress, format_speed, format_task_size, format_timestamp, has_active_tasks,
    sorted_task_indices, task_row, task_status_text, task_title, TaskSortColumn, TaskSortDirection,
};
use yt_dlp_gui::design_system::i18n::Locale;
use yt_dlp_gui::storage::{DownloadTask, DownloadTaskStatus};

fn task(
    id: i64,
    title: Option<&str>,
    status: DownloadTaskStatus,
    progress_percent: Option<u8>,
    total_bytes: Option<i64>,
    speed_bytes_per_second: Option<i64>,
    eta_seconds: Option<i64>,
    updated_at: i64,
    target_path: &str,
) -> DownloadTask {
    DownloadTask {
        id,
        source_url: format!("https://example.invalid/{id}"),
        video_id: None,
        title: title.map(str::to_owned),
        thumbnail_url: None,
        duration_seconds: None,
        target_path: target_path.to_owned(),
        output_path: None,
        selected_format: None,
        status,
        progress_percent,
        downloaded_bytes: total_bytes.map_or(0, |total| total / 2),
        total_bytes,
        total_bytes_estimate: None,
        speed_bytes_per_second,
        elapsed_seconds: None,
        eta_seconds,
        created_at: updated_at,
        started_at: None,
        finished_at: None,
        updated_at,
        yt_dlp_version: None,
        error_code: None,
        error_message: None,
    }
}

#[test]
fn task_rows_have_the_stable_eight_column_shape() {
    let download = task(
        7,
        Some("Example video"),
        DownloadTaskStatus::Downloading,
        Some(42),
        Some(2 * 1024 * 1024),
        Some(1024 * 1024),
        Some(65),
        1_700_000_000,
        "C:\\downloads",
    );
    let row = task_row(&download, Locale::EnUs);

    assert_eq!(row.cells.len(), 8);
    assert_eq!(row.cells[0], "Example video");
    assert_eq!(row.cells[1], "Downloading");
    assert_eq!(row.cells[2], "42%");
    assert_eq!(row.cells[3], "1.0 MiB / 2.0 MiB");
    assert_eq!(row.cells[4], "1.0 MiB/s");
    assert_eq!(row.cells[5], "1:05");
    assert_eq!(row.cells[6], "2023-11-14 22:13:20");
    assert_eq!(row.cells[7], "C:\\downloads");
}

#[test]
fn task_title_falls_back_to_source_url_for_missing_or_blank_titles() {
    let missing = task(
        1,
        None,
        DownloadTaskStatus::Pending,
        None,
        None,
        None,
        None,
        0,
        "target",
    );
    assert_eq!(task_title(&missing), "https://example.invalid/1");

    let blank = task(
        2,
        Some("  "),
        DownloadTaskStatus::Pending,
        None,
        None,
        None,
        None,
        0,
        "target",
    );
    assert_eq!(task_title(&blank), "https://example.invalid/2");
}

#[test]
fn optional_task_values_render_as_empty_strings() {
    let download = task(
        1,
        None,
        DownloadTaskStatus::Pending,
        None,
        None,
        None,
        None,
        0,
        "target",
    );
    let row = task_row(&download, Locale::ZhCn);

    assert_eq!(row.cells[1], "待开始");
    assert!(row.cells[2].is_empty());
    assert_eq!(row.cells[3], "0 B");
    assert!(row.cells[4].is_empty());
    assert!(row.cells[5].is_empty());
}

#[test]
fn status_text_has_the_stable_bilingual_mapping() {
    let statuses = [
        (DownloadTaskStatus::Pending, "待开始", "Pending"),
        (DownloadTaskStatus::Preparing, "准备中", "Preparing"),
        (DownloadTaskStatus::Downloading, "下载中", "Downloading"),
        (DownloadTaskStatus::Merging, "合并中", "Merging"),
        (DownloadTaskStatus::Completed, "已完成", "Completed"),
        (DownloadTaskStatus::Cancelled, "已取消", "Cancelled"),
        (DownloadTaskStatus::Failed, "失败", "Failed"),
    ];

    for (status, zh_cn, en_us) in statuses {
        assert_eq!(task_status_text(Locale::ZhCn, status), zh_cn);
        assert_eq!(task_status_text(Locale::EnUs, status), en_us);
    }
}

#[test]
fn task_sorting_uses_typed_values_and_places_unknown_values_last() {
    let tasks = vec![
        task(
            30,
            Some("Beta"),
            DownloadTaskStatus::Downloading,
            Some(8),
            Some(2_000),
            None,
            Some(120),
            30,
            "z",
        ),
        task(
            10,
            Some("alpha"),
            DownloadTaskStatus::Completed,
            Some(100),
            Some(1_000),
            Some(100),
            None,
            10,
            "a",
        ),
        task(
            20,
            Some("Gamma"),
            DownloadTaskStatus::Pending,
            None,
            None,
            Some(2_000),
            Some(60),
            20,
            "m",
        ),
    ];

    assert_eq!(
        sorted_task_indices(&tasks, Some(TaskSortColumn::Progress), TaskSortDirection::Ascending),
        vec![0, 1, 2]
    );
    assert_eq!(
        sorted_task_indices(&tasks, Some(TaskSortColumn::Speed), TaskSortDirection::Descending),
        vec![2, 1, 0]
    );
    assert_eq!(
        sorted_task_indices(&tasks, Some(TaskSortColumn::Title), TaskSortDirection::Ascending),
        vec![1, 0, 2]
    );
    assert_eq!(
        sorted_task_indices(&tasks, Some(TaskSortColumn::UpdatedAt), TaskSortDirection::Descending),
        vec![0, 2, 1]
    );
    assert_eq!(
        sorted_task_indices(&tasks, None, TaskSortDirection::Reset),
        vec![0, 1, 2]
    );
}

#[test]
fn checked_state_can_follow_source_indices_after_sorting() {
    let tasks = vec![
        task(
            2,
            Some("Beta"),
            DownloadTaskStatus::Pending,
            Some(20),
            Some(20),
            None,
            None,
            2,
            "b",
        ),
        task(
            1,
            Some("Alpha"),
            DownloadTaskStatus::Pending,
            Some(10),
            Some(10),
            None,
            None,
            1,
            "a",
        ),
    ];
    let order = sorted_task_indices(&tasks, Some(TaskSortColumn::Title), TaskSortDirection::Ascending);

    assert_eq!(order, vec![1, 0]);
    let mut checked = vec![false, true];
    let rows = order
        .iter()
        .map(|&index| task_row(&tasks[index], Locale::EnUs))
        .collect::<Vec<_>>();
    assert_eq!(rows[0].cells[0], "Alpha");
    assert_eq!(rows[1].cells[0], "Beta");
    assert!(checked[order[0]]);
    assert!(!checked[order[1]]);

    checked.fill(true);
    assert!(checked.iter().all(|&value| value));
}

#[test]
fn formatting_handles_epoch_and_invalid_values() {
    assert_eq!(format_timestamp(0), "1970-01-01 00:00:00");
    assert_eq!(format_timestamp(1_700_000_000), "2023-11-14 22:13:20");
    assert!(format_timestamp(-1).is_empty());
    assert_eq!(format_progress(Some(0)), "0%");
    assert!(format_progress(None).is_empty());
    assert_eq!(format_speed(Some(0)), "0 B/s");
    assert!(format_speed(Some(-1)).is_empty());
    assert_eq!(format_eta(Some(3_661)), "1:01:01");
    assert!(format_eta(Some(-1)).is_empty());

    let mut source = String::new();
    source.push_str(include_str!("../ui/pages/tasks-page.slint"));
    assert!(source.contains("GenericTable"));
    assert!(source.contains("show-check-column: true"));
    assert!(source.contains(
        "private property <length> task-table-max-height: 16 * Theme.control-min-height + 15 * Theme.spacing-compact;"
    ));
    assert!(source.contains("table-max-height: root.task-table-max-height"));
    assert!(source.contains("progress-column: 2"));
    assert!(source.contains("progress-values: root.progress-values"));
    assert!(source.contains("column-widths <=> root.column-widths"));
    assert!(source.contains("column-visibility <=> root.column-visibility"));
    assert!(source.contains("resizable-columns: true"));
    assert!(source.contains("column-hiding-enabled: true"));
    assert!(source.contains("menu-reset-widths-label: I18n.table-reset-widths"));
    assert!(source.contains("menu-reset-titles-label: I18n.table-reset-titles"));
    assert!(source.contains("menu-show-columns-label: I18n.table-show-columns"));
    assert!(source.contains("check-all-toggled(checked)"));
    assert!(!source.contains("Button"));
    assert!(!source.contains("ScrollView"));
    assert!(!source.contains("Flickable"));
}

#[test]
fn task_selection_callbacks_forward_check_all_and_keep_source_indices() {
    let page_source = include_str!("../ui/pages/tasks-page.slint");
    assert!(page_source.contains("callback check-all-toggled(bool);"));
    assert!(page_source.contains("root.check-all-toggled(checked);"));

    let app_window_source = include_str!("../ui/app-window.slint");
    assert!(app_window_source.contains("callback tasks-check-all-toggled(bool);"));
    assert!(app_window_source.contains("root.tasks-check-all-toggled(checked);"));

    let window_source = include_str!("../src/app/tasks_window.rs");
    assert!(window_source.contains("ui.on_tasks_check_all_toggled(move |checked|"));
    assert!(window_source.contains("state.checked.fill(checked);"));
    assert!(window_source.contains("state.checked.get_mut(source_row)"));
}

#[test]
fn task_page_uses_the_shared_theme_and_i18n_properties() {
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/pages/tasks-page.slint")).unwrap();
    for token in [
        "Theme.surface-primary",
        "Theme.border-default",
        "Theme.ui-font-family",
        "I18n.tasks-title",
        "I18n.tasks-no-tasks",
    ] {
        assert!(source.contains(token), "missing {token}");
    }
}

#[test]
fn task_sort_columns_cover_all_displayed_fields() {
    assert_eq!(TaskSortColumn::ALL.len(), 8);
    for (index, column) in TaskSortColumn::ALL.into_iter().enumerate() {
        assert_eq!(TaskSortColumn::from_index(index as i32), Some(column));
        assert_eq!(column.index(), index as i32);
    }
}

#[test]
fn task_size_formatter_uses_downloaded_and_total_values() {
    let download = task(
        1,
        Some("video"),
        DownloadTaskStatus::Completed,
        Some(100),
        Some(3 * 1024 * 1024),
        None,
        None,
        0,
        "target",
    );
    assert_eq!(format_task_size(&download), "1.5 MiB / 3.0 MiB");
}

#[test]
fn has_active_tasks_returns_true_when_any_task_is_not_terminal() {
    let active = [
        DownloadTaskStatus::Pending,
        DownloadTaskStatus::Preparing,
        DownloadTaskStatus::Downloading,
        DownloadTaskStatus::Merging,
    ];
    for status in active {
        let tasks = vec![task(1, Some("video"), status, None, None, None, None, 0, "target")];
        assert!(has_active_tasks(&tasks), "{status:?} 应为活动状态");
    }
}

#[test]
fn has_active_tasks_returns_false_for_empty_or_all_terminal_lists() {
    assert!(!has_active_tasks(&[]));

    let terminal = [
        DownloadTaskStatus::Completed,
        DownloadTaskStatus::Cancelled,
        DownloadTaskStatus::Failed,
    ];
    let tasks = terminal
        .into_iter()
        .enumerate()
        .map(|(id, status)| task(id as i64, Some("video"), status, None, None, None, None, 0, "target"))
        .collect::<Vec<_>>();
    assert!(!has_active_tasks(&tasks));

    let mixed = vec![
        task(
            1,
            Some("done"),
            DownloadTaskStatus::Completed,
            None,
            None,
            None,
            None,
            0,
            "target",
        ),
        task(
            2,
            Some("active"),
            DownloadTaskStatus::Downloading,
            None,
            None,
            None,
            None,
            0,
            "target",
        ),
        task(
            3,
            Some("failed"),
            DownloadTaskStatus::Failed,
            None,
            None,
            None,
            None,
            0,
            "target",
        ),
    ];
    assert!(has_active_tasks(&mixed));
}
