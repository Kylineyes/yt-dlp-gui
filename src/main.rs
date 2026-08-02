mod app;
mod download;
mod error;
mod fatal_error_window;
mod storage;

slint::include_modules!();

use app::settings_validation::{SettingsValidation, ValidationError, validate_settings};
use app::state::{AppSettings, DownloadRecord, MediaStream, NewDownload};
use app::{ErrorKind, NoticeKind, UiCommand, WorkerEvent};
use fatal_error_window::FatalErrorController;
use slint::{ComponentHandle, Model, ModelRc, SharedString, StyledText, TimerMode, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::mpsc::unbounded_channel;

const PAGE_WELCOME: i32 = 0;
const PAGE_SETTINGS: i32 = 3;
const TOAST_SUCCESS: i32 = 0;
const TOAST_ERROR: i32 = 1;
const TOAST_VALIDATION_ERROR: i32 = 2;

struct ToastController {
    model: Rc<VecModel<ToastData>>,
    active_page: Cell<i32>,
}

impl ToastController {
    fn new(model: Rc<VecModel<ToastData>>) -> Self {
        Self {
            model,
            active_page: Cell::new(PAGE_WELCOME),
        }
    }

    fn dismiss(&self, id: i32) -> bool {
        if let Some(index) = (0..self.model.row_count()).find(|&index| {
            self.model
                .row_data(index)
                .is_some_and(|toast| toast.id == id)
        }) {
            self.model.remove(index);
            true
        } else {
            false
        }
    }

    fn change_page(&self, page: i32) {
        if self.active_page.get() != page {
            self.active_page.set(page);
            self.model.set_vec(Vec::new());
        }
    }
}

type SharedToastController = Rc<ToastController>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = MainWindow::new()?;
    slint::select_bundled_translation("zh-CN")?;
    let (command_sender, command_receiver) = unbounded_channel();
    let (event_sender, event_receiver) = unbounded_channel();
    let worker = app::spawn_worker(command_receiver, event_sender);

    window.set_streams(ModelRc::new(VecModel::<StreamRow>::default()));
    window.set_downloads(ModelRc::new(VecModel::<DownloadRow>::default()));
    let toast_model = Rc::new(VecModel::<ToastData>::default());
    window.set_toasts(ModelRc::from(toast_model.clone()));
    let toasts = Rc::new(ToastController::new(toast_model));

    let fatal_window = Rc::new(RefCell::new(FatalErrorController::new(window.as_weak())));

    let _event_pump = install_event_pump(&window, event_receiver, fatal_window.clone());
    install_callbacks(&window, command_sender.clone(), toasts);

    window.run()?;
    fatal_window.borrow_mut().hide();
    let _ = command_sender.send(UiCommand::Shutdown);
    let _ = worker.join();
    Ok(())
}

fn install_event_pump(
    window: &MainWindow,
    mut event_receiver: tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
    fatal_window: Rc<RefCell<FatalErrorController>>,
) -> slint::Timer {
    let timer = slint::Timer::default();
    let weak_window = window.as_weak();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            while let Ok(event) = event_receiver.try_recv() {
                apply_event(&window, event, &fatal_window);
            }
            fatal_window.borrow_mut().ensure_native_modal();
        },
    );
    timer
}

fn apply_event(
    window: &MainWindow,
    event: WorkerEvent,
    fatal_window: &Rc<RefCell<FatalErrorController>>,
) {
    match event {
        WorkerEvent::Ready => {
            window.set_downloads_message_kind(1);
            window.set_downloads_message_argument("".into());
        }
        WorkerEvent::StorageInitializationFailed { detail } => {
            let is_new = !fatal_window.borrow().is_visible();
            fatal_window.borrow_mut().show_or_update(detail);
            fatal_window.borrow_mut().ensure_native_modal();
            if is_new {
                install_fatal_callbacks(fatal_window.clone());
            }
        }
        WorkerEvent::SettingsLoaded(settings) => apply_settings(window, &settings),
        WorkerEvent::SettingsSaved(settings) => {
            apply_settings(window, &settings);
            window.set_settings_message_kind(0);
            window.set_settings_message_argument("".into());
            show_toast(window, PAGE_SETTINGS, TOAST_SUCCESS, "");
        }
        WorkerEvent::SearchCompleted(result) => {
            window.set_resource_name(result.resource_name.into());
            let rows = result
                .streams
                .iter()
                .enumerate()
                .map(|(index, stream)| stream_to_row(stream, index == 0))
                .collect::<Vec<_>>();
            window.set_streams(ModelRc::from(Rc::new(VecModel::from(rows))));
            window.set_search_message_kind(2);
            window.set_search_message_argument("".into());
        }
        WorkerEvent::SearchFailed { detail } => {
            window.set_resource_name("".into());
            window.set_streams(ModelRc::new(VecModel::<StreamRow>::default()));
            window.set_search_message_kind(8);
            window.set_search_message_argument(detail.into());
        }
        WorkerEvent::DownloadsLoaded(downloads) => {
            let rows = downloads
                .into_iter()
                .map(download_to_row)
                .collect::<Vec<_>>();
            let count = rows.len();
            window.set_downloads(ModelRc::from(Rc::new(VecModel::from(rows))));
            window.set_downloads_message_kind(2);
            window.set_downloads_message_argument(count.to_string().into());
        }
        WorkerEvent::DownloadStarted { id } => {
            window.set_selected_page(2);
            window.set_downloads_message_kind(3);
            window.set_downloads_message_task_id(i32::try_from(id).unwrap_or(i32::MAX));
            window.set_downloads_message_argument("".into());
            window.set_downloads_message_detail("".into());
        }
        WorkerEvent::LogLine { id, line } => {
            window.set_downloads_message_kind(4);
            window.set_downloads_message_task_id(i32::try_from(id).unwrap_or(i32::MAX));
            window.set_downloads_message_detail(line.into());
        }
        WorkerEvent::DownloadFinished { id } => {
            window.set_downloads_message_kind(5);
            window.set_downloads_message_task_id(i32::try_from(id).unwrap_or(i32::MAX));
            window.set_downloads_message_detail("".into());
        }
        WorkerEvent::DownloadFailed { id, message } => {
            window.set_downloads_message_kind(6);
            window.set_downloads_message_task_id(i32::try_from(id).unwrap_or(i32::MAX));
            window.set_downloads_message_detail(message.into());
        }
        WorkerEvent::Notice { kind, argument } => {
            window.set_downloads_message_kind(match kind {
                NoticeKind::PauseUnsupported => 7,
            });
            window.set_downloads_message_task_id(argument.parse().unwrap_or_default());
            window.set_downloads_message_argument("".into());
            window.set_downloads_message_detail("".into());
        }
        WorkerEvent::Error { kind, detail } => match kind {
            ErrorKind::General => {
                let page = window.get_selected_page();
                show_toast(window, page, TOAST_ERROR, &detail);
            }
        },
    }
}

fn install_fatal_callbacks(controller: Rc<RefCell<FatalErrorController>>) {
    controller.borrow().with_window(|window| {
        let callback_controller = controller.clone();
        window.on_toggle_fatal_detail(move || {
            callback_controller.borrow().with_window(|window| {
                window.set_fatal_detail_expanded(!window.get_fatal_detail_expanded());
            });
        });
        let callback_controller = controller.clone();
        window.on_confirm_fatal_error(move || {
            callback_controller.borrow_mut().hide();
            let _ = slint::quit_event_loop();
        });
    });
}

fn install_callbacks(
    window: &MainWindow,
    commands: tokio::sync::mpsc::UnboundedSender<UiCommand>,
    toasts: SharedToastController,
) {
    window.on_format_step_description(|description| {
        StyledText::from_markdown(description.as_str())
            .unwrap_or_else(|_| StyledText::from_plain_text(description.as_str()))
    });

    let callback_toasts = toasts.clone();
    window.on_toast_dismissed(move |id| {
        callback_toasts.dismiss(id);
    });

    let callback_toasts = toasts.clone();
    window.on_page_changed(move |page| callback_toasts.change_page(page));

    let weak = window.as_weak();
    window.on_save_settings(move || {
        if let Some(window) = weak.upgrade() {
            window.set_pending_save(true);
            window.invoke_validate_settings(-1);
        }
    });

    let weak = window.as_weak();
    window.on_change_language(move |language| {
        if let Some(window) = weak.upgrade() {
            let language = language.to_string();
            match slint::select_bundled_translation(&language) {
                Ok(()) => window.set_language(language.into()),
                Err(error) => show_toast(&window, PAGE_SETTINGS, TOAST_ERROR, &error.to_string()),
            }
        }
    });

    let weak = window.as_weak();
    window.on_reset_settings(move || {
        if let Some(window) = weak.upgrade() {
            apply_settings(&window, &AppSettings::default());
            window.set_settings_message_kind(2);
            window.set_settings_message_argument("".into());
        }
    });

    let weak = window.as_weak();
    window.on_select_yt_dlp(move |title| {
        if let Some(window) = weak.upgrade()
            && let Some(path) = rfd::FileDialog::new().set_title(title.as_str()).pick_file()
        {
            window.set_yt_dlp_path(path.to_string_lossy().into_owned().into());
            window.invoke_validate_settings(0);
        }
    });

    let weak = window.as_weak();
    window.on_select_ffmpeg(move |title| {
        if let Some(window) = weak.upgrade()
            && let Some(path) = rfd::FileDialog::new().set_title(title.as_str()).pick_file()
        {
            window.set_ffmpeg_path(path.to_string_lossy().into_owned().into());
            window.invoke_validate_settings(1);
        }
    });

    let weak = window.as_weak();
    window.on_select_download_directory(move |title| {
        if let Some(window) = weak.upgrade()
            && let Some(path) = rfd::FileDialog::new()
                .set_title(title.as_str())
                .pick_folder()
        {
            window.set_default_download_directory(path.to_string_lossy().into_owned().into());
            window.invoke_validate_settings(2);
        }
    });

    install_settings_validation(window, commands.clone());

    let weak = window.as_weak();
    let sender = commands.clone();
    window.on_search(move || {
        if let Some(window) = weak.upgrade() {
            let url = window.get_search_url().to_string();
            if url.trim().is_empty() {
                window.set_search_message_kind(1);
                window.set_search_message_argument("".into());
            } else {
                window.set_resource_name("".into());
                window.set_streams(ModelRc::new(VecModel::<StreamRow>::default()));
                window.set_search_message_kind(6);
                window.set_search_message_argument("".into());
                let _ = sender.send(UiCommand::Search { url });
            }
        }
    });

    let weak = window.as_weak();
    window.on_select_stream(move |selected_index| {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let model = window.get_streams();
        for index in 0..model.row_count() {
            if let Some(mut row) = model.row_data(index) {
                row.selected = index == selected_index as usize;
                model.set_row_data(index, row);
            }
        }
        window.set_search_message_kind(0);
        window.set_search_message_argument("".into());
    });

    let weak = window.as_weak();
    let sender = commands.clone();
    window.on_create_download(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        let model = window.get_streams();
        let selected = (0..model.row_count())
            .filter_map(|index| model.row_data(index))
            .find(|row| row.selected);
        let Some(stream) = selected else {
            window.set_search_message_kind(4);
            window.set_search_message_argument("".into());
            return;
        };
        let output_directory = window.get_search_output_directory().to_string();
        if output_directory.trim().is_empty() {
            window.set_search_message_kind(5);
            window.set_search_message_argument("".into());
            return;
        }
        let download = NewDownload {
            url: window.get_search_url().to_string(),
            resource_name: window.get_resource_name().to_string(),
            output_directory,
            format_selector: stream.format_selector.to_string(),
            video_format: stream.video_format.to_string(),
            audio_format: stream.audio_format.to_string(),
        };
        if sender.send(UiCommand::CreateDownload(download)).is_ok() {
            window.set_selected_page(2);
            window.set_downloads_message_kind(8);
            window.set_downloads_message_argument("".into());
        }
    });

    let sender = commands.clone();
    window.on_refresh_downloads(move || {
        let _ = sender.send(UiCommand::RefreshDownloads);
    });

    let sender = commands.clone();
    window.on_start_download(move |id| {
        let _ = sender.send(UiCommand::StartDownload { id: i64::from(id) });
    });

    let sender = commands.clone();
    window.on_pause_download(move |id| {
        let _ = sender.send(UiCommand::PauseDownload { id: i64::from(id) });
    });

    let sender = commands.clone();
    window.on_window_closed(move || {
        let _ = sender.send(UiCommand::Shutdown);
    });

    let _ = commands.send(UiCommand::LoadSettings);
}

fn install_settings_validation(
    window: &MainWindow,
    commands: tokio::sync::mpsc::UnboundedSender<UiCommand>,
) {
    let revision = Arc::new(AtomicU64::new(0));
    let weak = window.as_weak();
    let callback_revision = revision.clone();
    let callback_commands = commands.clone();

    window.on_validate_settings(move |_| {
        let current_revision = callback_revision.fetch_add(1, Ordering::SeqCst) + 1;
        let weak = weak.clone();
        let revision = callback_revision.clone();
        let Some(window) = weak.upgrade() else {
            return;
        };
        let settings = settings_from_window(&window);
        let weak = window.as_weak();
        let revision = revision.clone();
        let commands = callback_commands.clone();
        std::thread::spawn(move || {
            let validation = validate_settings(&settings);
            let _ = slint::invoke_from_event_loop(move || {
                if revision.load(Ordering::SeqCst) != current_revision {
                    return;
                }
                if let Some(window) = weak.upgrade() {
                    let should_save = window.get_pending_save();
                    apply_validation(&window, validation);
                    if should_save && validation.is_valid() {
                        let settings = settings_from_window(&window);
                        if commands.send(UiCommand::SaveSettings(settings)).is_err() {
                            show_toast(
                                &window,
                                PAGE_SETTINGS,
                                TOAST_ERROR,
                                "Settings command channel is closed",
                            );
                        }
                    }
                }
            });
        });
    });

    // The callback owns a clone; this initial invocation validates values loaded at startup.
    window.invoke_validate_settings(-1);
}

fn apply_validation(window: &MainWindow, validation: SettingsValidation) {
    if window.get_pending_save() {
        if let Some(field) = validation.first_invalid_field() {
            window.set_pending_save(false);
            window.set_selected_page(3);
            window.set_invalid_setting_field(field);
            let revision = window
                .get_invalid_setting_revision()
                .checked_add(1)
                .unwrap_or(1);
            window.set_invalid_setting_revision(revision);
            show_toast(window, PAGE_SETTINGS, TOAST_VALIDATION_ERROR, "");
        } else {
            window.set_pending_save(false);
        }
    }
    window.set_yt_dlp_status(validation_status(validation.yt_dlp_error));
    window.set_ffmpeg_status(validation_status(validation.ffmpeg_error));
    window.set_download_directory_status(validation_status(validation.download_directory_error));
    window.set_proxy_status(validation_status(validation.proxy_error));
    window.set_yt_dlp_error_kind(validation_error_kind(validation.yt_dlp_error));
    window.set_ffmpeg_error_kind(validation_error_kind(validation.ffmpeg_error));
    window.set_download_directory_error_kind(validation_error_kind(
        validation.download_directory_error,
    ));
    window.set_proxy_error_kind(validation_error_kind(validation.proxy_error));
}

fn validation_status(error: ValidationError) -> ValidationStatus {
    if error == ValidationError::None {
        ValidationStatus::Valid
    } else {
        ValidationStatus::Invalid
    }
}

fn validation_error_kind(error: ValidationError) -> i32 {
    match error {
        ValidationError::None => 0,
        ValidationError::SurroundingWhitespace => 1,
        ValidationError::InvalidExecutablePath | ValidationError::InvalidDirectory => 2,
        ValidationError::ExecutableProbeFailed => 3,
    }
}

fn show_toast(window: &MainWindow, page: i32, kind: i32, detail: &str) {
    if window.get_selected_page() != page {
        return;
    }
    let model = window.get_toasts();
    let model = model
        .as_any()
        .downcast_ref::<VecModel<ToastData>>()
        .expect("Toast model should be a VecModel");
    let next_id = model
        .iter()
        .map(|toast| toast.id)
        .max()
        .unwrap_or_default()
        .checked_add(1)
        .unwrap_or(1);
    model.push(ToastData {
        id: next_id,
        kind,
        detail: detail.into(),
        page,
    });
}

fn settings_from_window(window: &MainWindow) -> AppSettings {
    AppSettings {
        yt_dlp_path: window.get_yt_dlp_path().to_string(),
        ffmpeg_path: window.get_ffmpeg_path().to_string(),
        default_download_directory: window.get_default_download_directory().to_string(),
        proxy: window.get_proxy().to_string(),
        max_concurrency: window.get_max_concurrency().clamp(1, 16) as u32,
        language: window.get_language().to_string(),
    }
}

fn apply_settings(window: &MainWindow, settings: &AppSettings) {
    window.set_yt_dlp_path(settings.yt_dlp_path.clone().into());
    window.set_ffmpeg_path(settings.ffmpeg_path.clone().into());
    window.set_default_download_directory(settings.default_download_directory.clone().into());
    window.set_search_output_directory(settings.default_download_directory.clone().into());
    window.set_proxy(settings.proxy.clone().into());
    window.set_max_concurrency(settings.max_concurrency as i32);
    if slint::select_bundled_translation(&settings.language).is_ok() {
        window.set_language(settings.language.clone().into());
    }
    window.invoke_validate_settings(-1);
}

fn stream_to_row(stream: &MediaStream, selected: bool) -> StreamRow {
    StreamRow {
        id: SharedString::from(&stream.id),
        label: SharedString::from(&stream.label),
        format_selector: SharedString::from(&stream.format_selector),
        video_format: SharedString::from(&stream.video_format),
        audio_format: SharedString::from(&stream.audio_format),
        estimated_size: SharedString::from(&stream.estimated_size),
        selected,
    }
}

fn download_to_row(record: DownloadRecord) -> DownloadRow {
    let progress = if record.total_bytes > 0 {
        record.downloaded_bytes as f32 / record.total_bytes as f32
    } else if record.status == app::state::DownloadStatus::Completed {
        1.0
    } else {
        0.0
    };
    DownloadRow {
        id: i32::try_from(record.id).unwrap_or(i32::MAX),
        name: record.resource_name.into(),
        url: record.url.into(),
        status: record.status.as_str().into(),
        started_at: format_timestamp(record.started_at).into(),
        completed_at: format_timestamp(record.completed_at).into(),
        output_directory: value_or(&record.output_path, &record.output_directory).into(),
        size_text: format_size(record.downloaded_bytes, record.total_bytes).into(),
        format_text: format!("{} / {}", record.video_format, record.audio_format).into(),
        progress,
        error_message: record.error_message.into(),
    }
}

fn value_or<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn format_timestamp(value: Option<i64>) -> String {
    value.map_or_else(|| "—".into(), |timestamp| timestamp.to_string())
}

fn format_size(downloaded: i64, total: i64) -> String {
    let downloaded = human_bytes(downloaded);
    if total > 0 {
        format!("{downloaded} / {}", human_bytes(total))
    } else {
        format!("{downloaded} / ?")
    }
}

fn human_bytes(bytes: i64) -> String {
    let bytes = bytes.max(0) as f64;
    for (unit, scale) in [("GB", 1_073_741_824.0), ("MB", 1_048_576.0), ("KB", 1024.0)] {
        if bytes >= scale {
            return format!("{:.1} {unit}", bytes / scale);
        }
    }
    format!("{} B", bytes as i64)
}
