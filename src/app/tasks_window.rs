use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::tasks::{self, TaskSortColumn, TaskSortDirection};
use super::{AppWindow, TableRow as SlintTableRow};
use crate::app::contracts::Route;
use crate::design_system::i18n::Locale;
use crate::download_task::{DownloadOptions, DownloadRequest, DownloadTaskClient, MediaFormat, VideoInfo};
use crate::storage::{DownloadStreamMediaType, DownloadTask, DownloadTaskFilter, DownloadTaskStream, Storage};

const TASK_ACTION_OPEN_PATH: i32 = 0;
const TASK_ACTION_DELETE: i32 = 1;
const TASK_ACTION_REDOWNLOAD: i32 = 2;
const TASK_ACTION_OPEN_URL: i32 = 3;
const TASK_ACTION_SELECT_ALL: i32 = 4;

struct TasksState {
    tasks: Vec<DownloadTask>,
    checked: Vec<bool>,
    selected_task_id: Option<i64>,
    sort_column: Option<TaskSortColumn>,
    sort_direction: TaskSortDirection,
    timers: Vec<slint::Timer>,
}

pub(super) fn install(ui: &AppWindow, storage: &'static Storage, locale: Rc<Cell<Locale>>) -> Rc<dyn Fn()> {
    let state = Rc::new(RefCell::new(TasksState {
        tasks: Vec::new(),
        checked: Vec::new(),
        selected_task_id: None,
        sort_column: None,
        sort_direction: TaskSortDirection::Reset,
        timers: Vec::new(),
    }));

    let refresh: Rc<dyn Fn()> = {
        let state = Rc::clone(&state);
        let ui_weak = ui.as_weak();
        let locale = Rc::clone(&locale);
        Rc::new(move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            reload(&ui, storage, &state, locale.get());
        })
    };

    let poll_state = Rc::clone(&state);
    let poll_ui = ui.as_weak();
    let poll_locale = Rc::clone(&locale);
    let poll_timer = slint::Timer::default();
    poll_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = poll_ui.upgrade() else { return };
        if ui.get_current_route() != Route::Tasks.index() {
            return;
        }
        if !tasks::has_active_tasks(&poll_state.borrow().tasks) {
            return;
        }
        reload(&ui, storage, &poll_state, poll_locale.get());
    });
    state.borrow_mut().timers.push(poll_timer);

    let sort_state = Rc::clone(&state);
    let sort_ui = ui.as_weak();
    let sort_locale = Rc::clone(&locale);
    ui.on_tasks_sort_requested(move |column, direction| {
        let Some(column) = TaskSortColumn::from_index(column) else {
            return;
        };
        let Some(direction) = TaskSortDirection::from_index(direction) else {
            return;
        };
        let Some(ui) = sort_ui.upgrade() else { return };
        {
            let mut state = sort_state.borrow_mut();
            state.sort_column = (direction != TaskSortDirection::Reset).then_some(column);
            state.sort_direction = direction;
            render(&ui, &state, sort_locale.get());
        }
    });

    let selection_state = Rc::clone(&state);
    let selection_ui = ui.as_weak();
    let selection_locale = Rc::clone(&locale);
    ui.on_tasks_row_selected(move |source_row| {
        let Ok(source_row) = usize::try_from(source_row) else {
            return;
        };
        let Some(ui) = selection_ui.upgrade() else { return };
        let mut state = selection_state.borrow_mut();
        let Some(task) = state.tasks.get(source_row) else {
            return;
        };
        state.selected_task_id = Some(task.id);
        render(&ui, &state, selection_locale.get());
    });

    let check_state = Rc::clone(&state);
    let check_ui = ui.as_weak();
    let check_locale = Rc::clone(&locale);
    ui.on_tasks_check_toggled(move |source_row, checked| {
        let Ok(source_row) = usize::try_from(source_row) else {
            return;
        };
        let Some(ui) = check_ui.upgrade() else { return };
        let mut state = check_state.borrow_mut();
        let Some(value) = state.checked.get_mut(source_row) else {
            return;
        };
        *value = checked;
        render(&ui, &state, check_locale.get());
    });

    let delete_state = Rc::clone(&state);
    let delete_refresh = Rc::clone(&refresh);
    ui.on_tasks_delete_selected_requested(move || {
        let ids = {
            let state = delete_state.borrow();
            state
                .tasks
                .iter()
                .zip(&state.checked)
                .filter_map(|(task, &checked)| checked.then_some(task.id))
                .collect::<Vec<_>>()
        };
        if ids.is_empty() {
            return;
        }
        if let Err(error) = storage.delete_download_tasks(&ids) {
            eprintln!("删除下载任务历史失败：{error}");
            return;
        }
        delete_refresh();
    });

    let action_state = Rc::clone(&state);
    let action_ui = ui.as_weak();
    let action_refresh = Rc::clone(&refresh);
    let action_locale = Rc::clone(&locale);
    ui.on_tasks_row_action_requested(move |source_row, action| {
        if action == TASK_ACTION_SELECT_ALL {
            let Some(ui) = action_ui.upgrade() else { return };
            let mut state = action_state.borrow_mut();
            state.checked.fill(true);
            render(&ui, &state, action_locale.get());
            return;
        }
        let Ok(source_row) = usize::try_from(source_row) else {
            return;
        };
        let Some(task) = action_state.borrow().tasks.get(source_row).cloned() else {
            return;
        };
        match action {
            TASK_ACTION_OPEN_PATH => open_task_video_path(&task),
            TASK_ACTION_DELETE => {
                if let Err(error) = storage.delete_download_tasks(&[task.id]) {
                    eprintln!("删除下载任务历史失败：{error}");
                } else {
                    action_refresh();
                }
            }
            TASK_ACTION_REDOWNLOAD => {
                if let Err(error) = redownload_task(storage, &task) {
                    eprintln!("重新下载任务失败：{error}");
                }
                action_refresh();
            }
            TASK_ACTION_OPEN_URL => {
                if let Err(error) = webbrowser::open(&task.source_url) {
                    eprintln!("打开视频链接失败：{error}");
                }
            }
            _ => {}
        }
    });

    let check_all_state = Rc::clone(&state);
    let check_all_ui = ui.as_weak();
    let check_all_locale = Rc::clone(&locale);
    ui.on_tasks_check_all_toggled(move |checked| {
        let Some(ui) = check_all_ui.upgrade() else { return };
        let mut state = check_all_state.borrow_mut();
        state.checked.fill(checked);
        render(&ui, &state, check_all_locale.get());
    });

    refresh();
    refresh
}

fn reload(ui: &AppWindow, storage: &'static Storage, state: &Rc<RefCell<TasksState>>, locale: Locale) {
    let tasks = match storage.list_download_tasks(DownloadTaskFilter::default()) {
        Ok(tasks) => tasks,
        Err(error) => {
            eprintln!("读取下载任务历史失败：{error}");
            return;
        }
    };

    let mut state = state.borrow_mut();
    let checked_ids = state
        .tasks
        .iter()
        .zip(&state.checked)
        .filter_map(|(task, &checked)| checked.then_some(task.id))
        .collect::<Vec<_>>();
    state.tasks = tasks;
    state.checked = state.tasks.iter().map(|task| checked_ids.contains(&task.id)).collect();
    if state
        .selected_task_id
        .is_some_and(|selected_id| !state.tasks.iter().any(|task| task.id == selected_id))
    {
        state.selected_task_id = None;
    }
    render(ui, &state, locale);
}

fn render(ui: &AppWindow, state: &TasksState, locale: Locale) {
    let order = tasks::sorted_task_indices(&state.tasks, state.sort_column, state.sort_direction);
    let selected_source_row = state.selected_task_id.and_then(|selected_id| {
        order
            .iter()
            .find(|&&source_index| state.tasks[source_index].id == selected_id)
            .map(|&source_index| source_index as i32)
    });
    let mut progress_values = Vec::with_capacity(order.len());
    let rows = order
        .into_iter()
        .filter_map(|source_index| {
            let task = state.tasks.get(source_index)?;
            progress_values.push(i32::from(task.progress_percent.unwrap_or(0)));
            let row = tasks::task_row(task, locale);
            let cells = row.cells.into_iter().map(SharedString::from).collect::<Vec<_>>();
            Some(SlintTableRow {
                source_index: source_index as i32,
                cells: ModelRc::new(VecModel::from(cells)),
                checked: state.checked.get(source_index).copied().unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();

    ui.set_tasks_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_tasks_progress_values(ModelRc::new(VecModel::from(progress_values)));
    ui.set_tasks_selected_source_row(selected_source_row.unwrap_or(-1));
    ui.set_tasks_sort_column(state.sort_column.map_or(-1, TaskSortColumn::index));
    ui.set_tasks_sort_direction(state.sort_direction.index());
}

fn open_task_video_path(task: &DownloadTask) {
    let output_path = task.output_path.as_deref().map(PathBuf::from);
    let target_path = PathBuf::from(&task.target_path);

    #[cfg(windows)]
    {
        let result = output_path.filter(|path| path.is_file()).map_or_else(
            || Command::new("explorer.exe").arg(&target_path).spawn(),
            |path| Command::new("explorer.exe").arg("/select,").arg(path).spawn(),
        );
        if let Err(error) = result {
            eprintln!("打开视频存放路径失败：{error}");
        }
    }

    #[cfg(target_os = "macos")]
    {
        let result = output_path.filter(|path| path.is_file()).map_or_else(
            || Command::new("open").arg(&target_path).spawn(),
            |path| Command::new("open").args(["-R", &path.to_string_lossy()]).spawn(),
        );
        if let Err(error) = result {
            eprintln!("打开视频存放路径失败：{error}");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Err(error) = Command::new("xdg-open").arg(&target_path).spawn() {
            eprintln!("打开视频存放路径失败：{error}");
        }
    }
}

fn redownload_task(storage: &'static Storage, task: &DownloadTask) -> Result<(), String> {
    if !task.status.is_terminal() {
        return Err("活动任务不能重新下载".to_owned());
    }
    let stored = storage
        .get_download_task(task.id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "下载任务不存在".to_owned())?;
    let request = download_request_from_task(&stored.task, &stored.streams)?;
    let configuration = storage
        .configuration()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "配置数据库中不存在环境配置记录".to_owned())?;
    let target_directory = PathBuf::from(&stored.task.target_path);
    std::fs::create_dir_all(&target_directory).map_err(|error| format!("创建下载目录失败：{error}"))?;
    let temporary_directory = target_directory.join(".yt-dlp-gui-temp");
    std::fs::create_dir_all(&temporary_directory).map_err(|error| format!("创建下载临时目录失败：{error}"))?;
    let ffmpeg_path = (!configuration.ffmpeg_path.trim().is_empty()).then(|| configuration.ffmpeg_path.into());
    let client = DownloadTaskClient::new(
        configuration.yt_dlp_path,
        ffmpeg_path,
        Some(configuration.proxy),
        Duration::ZERO,
        target_directory,
    );
    client.verify_version().map_err(|error| error.to_string())?;
    storage
        .delete_download_tasks(&[stored.task.id])
        .map_err(|error| error.to_string())?;
    let handle = client.download(request, |_| {}).map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        if let Err(error) = handle.wait() {
            eprintln!("重新下载任务失败：{error}");
        }
    });
    Ok(())
}

fn download_request_from_task(task: &DownloadTask, streams: &[DownloadTaskStream]) -> Result<DownloadRequest, String> {
    let video_stream = streams
        .iter()
        .find(|stream| stream.media_type == DownloadStreamMediaType::Video)
        .ok_or_else(|| "任务缺少视频流记录".to_owned())?;
    let audio_stream = streams
        .iter()
        .find(|stream| stream.media_type == DownloadStreamMediaType::Audio)
        .ok_or_else(|| "任务缺少音频流记录".to_owned())?;
    let selected_video_format_id = stream_format_id(video_stream)?;
    let selected_audio_format_id = stream_format_id(audio_stream)?;
    let video = VideoInfo {
        id: task.video_id.clone().unwrap_or_default(),
        title: task.title.clone().unwrap_or_default(),
        webpage_url: Some(task.source_url.clone()),
        original_url: Some(task.source_url.clone()),
        uploader: None,
        channel: None,
        duration_seconds: task.duration_seconds.map(|value| value as f64),
        thumbnail_url: task.thumbnail_url.clone(),
        description: None,
        upload_date: None,
        formats: vec![
            stream_to_format(video_stream, &selected_video_format_id),
            stream_to_format(audio_stream, &selected_audio_format_id),
        ],
    };
    Ok(DownloadRequest {
        source_url: task.source_url.clone(),
        video,
        selected_video_format_id,
        selected_audio_format_id,
        output_template: "%(title).80B [%(id)s].%(ext)s".to_owned(),
        target_directory: PathBuf::from(&task.target_path),
        temporary_directory: PathBuf::from(&task.target_path).join(".yt-dlp-gui-temp"),
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions::default(),
    })
}

fn stream_format_id(stream: &DownloadTaskStream) -> Result<String, String> {
    stream
        .format_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| (!stream.stream_key.trim().is_empty()).then(|| stream.stream_key.clone()))
        .ok_or_else(|| "任务流缺少格式 ID".to_owned())
}

fn stream_to_format(stream: &DownloadTaskStream, format_id: &str) -> MediaFormat {
    MediaFormat {
        format_id: Some(format_id.to_owned()),
        format_note: None,
        extension: stream.extension.clone(),
        resolution: None,
        width: stream.width.and_then(|value| u64::try_from(value).ok()),
        height: stream.height.and_then(|value| u64::try_from(value).ok()),
        fps: None,
        filesize: None,
        filesize_approx: None,
        bitrate_kbps: None,
        video_codec: stream.video_codec.clone(),
        audio_codec: stream.audio_codec.clone(),
        audio_bitrate_kbps: None,
        video_bitrate_kbps: None,
        protocol: None,
        url: None,
    }
}
