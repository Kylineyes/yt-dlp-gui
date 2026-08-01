mod app;
mod download;
mod error;
mod storage;

slint::include_modules!();

use app::state::{AppSettings, DownloadRecord, MediaStream, NewDownload};
use app::{ErrorKind, NoticeKind, UiCommand, WorkerEvent};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use tokio::sync::mpsc::unbounded_channel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = MainWindow::new()?;
    slint::select_bundled_translation("zh-CN")?;
    let (command_sender, command_receiver) = unbounded_channel();
    let (event_sender, event_receiver) = unbounded_channel();
    let worker = app::spawn_worker(command_receiver, event_sender);

    window.set_streams(ModelRc::new(VecModel::<StreamRow>::default()));
    window.set_downloads(ModelRc::new(VecModel::<DownloadRow>::default()));

    install_event_bridge(&window, event_receiver);
    install_callbacks(&window, command_sender.clone());

    window.run()?;
    let _ = command_sender.send(UiCommand::Shutdown);
    let _ = worker.join();
    Ok(())
}

fn install_event_bridge(
    window: &MainWindow,
    mut event_receiver: tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
) {
    let weak_window = window.as_weak();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create the event forwarding runtime");
        runtime.block_on(async move {
            while let Some(event) = event_receiver.recv().await {
                let weak_window = weak_window.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = weak_window.upgrade() else {
                        return;
                    };
                    apply_event(&window, event);
                });
            }
        });
    });
}

fn apply_event(window: &MainWindow, event: WorkerEvent) {
    match event {
        WorkerEvent::Ready => {
            window.set_downloads_message_kind(1);
            window.set_downloads_message_argument("".into());
        }
        WorkerEvent::SettingsLoaded(settings) => apply_settings(window, &settings),
        WorkerEvent::SettingsSaved(settings) => {
            apply_settings(window, &settings);
            window.set_settings_message_kind(1);
            window.set_settings_message_argument("".into());
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
        WorkerEvent::Error { kind, detail } => {
            let kind = match kind {
                ErrorKind::General => 1,
            };
            window.set_settings_message_kind(kind);
            window.set_settings_message_argument(detail.clone().into());
            window.set_search_message_kind(kind);
            window.set_search_message_argument(detail.clone().into());
            window.set_downloads_message_kind(kind);
            window.set_downloads_message_argument(detail.into());
        }
    }
}

fn install_callbacks(window: &MainWindow, commands: tokio::sync::mpsc::UnboundedSender<UiCommand>) {
    let weak = window.as_weak();
    let sender = commands.clone();
    window.on_save_settings(move || {
        if let Some(window) = weak.upgrade() {
            let settings = settings_from_window(&window);
            if sender.send(UiCommand::SaveSettings(settings)).is_err() {
                window.set_settings_message_kind(3);
                window.set_settings_message_argument("".into());
            }
        }
    });

    let weak = window.as_weak();
    window.on_change_language(move |language| {
        if let Some(window) = weak.upgrade() {
            let language = language.to_string();
            match slint::select_bundled_translation(&language) {
                Ok(()) => window.set_language(language.into()),
                Err(error) => {
                    window.set_settings_message_kind(0);
                    window.set_settings_message_argument(error.to_string().into());
                }
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
    let sender = commands.clone();
    window.on_search(move || {
        if let Some(window) = weak.upgrade() {
            let url = window.get_search_url().to_string();
            if url.trim().is_empty() {
                window.set_search_message_kind(1);
                window.set_search_message_argument("".into());
            } else {
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
        window.set_search_message_kind(3);
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
