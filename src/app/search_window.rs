use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, VecModel};

use super::{AppWindow, SearchResultRow};
use crate::app::search::{
    can_download, classify_failure, next_sort_state, result_rows_in_order, sorted_result_indices,
    validate_download_path, SearchFailure, SearchPathError, SortColumn, SortDirection,
};
use crate::design_system::i18n::{I18nCatalog, Locale, TextKey};
use crate::download_task::{DownloadTaskClient, DownloadTaskError, MediaMessage, VideoInfo, DEFAULT_METADATA_TIMEOUT};
use crate::storage::Storage;

#[derive(Debug)]
enum SearchEvent {
    Message(MediaMessage),
    Completion(Result<VideoInfo, DownloadTaskError>),
}

struct SearchState {
    cancelled: Option<Arc<AtomicBool>>,
    receiver: Option<Receiver<SearchEvent>>,
    metadata: Option<VideoInfo>,
    started_at: Option<Instant>,
    path_error: Option<SearchPathError>,
    selected_index: Option<usize>,
    selected_original_index: Option<usize>,
    sort_column: Option<SortColumn>,
    sort_direction: SortDirection,
    visible_order: Vec<usize>,
    timers: Vec<slint::Timer>,
}

pub(super) fn install(ui: &AppWindow, storage: &'static Storage, locale: Rc<Cell<Locale>>) {
    let state = Rc::new(RefCell::new(SearchState {
        cancelled: None,
        receiver: None,
        metadata: None,
        started_at: None,
        path_error: None,
        selected_index: None,
        selected_original_index: None,
        sort_column: None,
        sort_direction: SortDirection::Reset,
        visible_order: Vec::new(),
        timers: Vec::new(),
    }));

    if let Ok(Some(path)) = storage.default_download_path() {
        state.borrow_mut().path_error = validate_download_path(&path).err();
        ui.set_search_download_path(path.into());
        ui.set_search_path_error(path_error_text(locale.get(), state.borrow().path_error).into());
    }
    ui.set_search_results(ModelRc::new(VecModel::from(Vec::<SearchResultRow>::new())));

    install_url_edit(ui, Rc::clone(&state));
    install_path_edit(ui, Rc::clone(&state), Rc::clone(&locale));
    install_browse(ui, Rc::clone(&state), Rc::clone(&locale));
    install_default_path(ui, storage, Rc::clone(&state), Rc::clone(&locale));
    install_search(ui, storage, Rc::clone(&state), Rc::clone(&locale));
    install_stop(ui, Rc::clone(&state));
    install_result_selection(ui, Rc::clone(&state));
    install_result_sort(ui, Rc::clone(&state));

    let poll_state = Rc::clone(&state);
    let poll_ui = ui.as_weak();
    let poll_locale = Rc::clone(&locale);
    let poll_timer = slint::Timer::default();
    poll_timer.start(slint::TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = poll_ui.upgrade() else { return };
        poll_events(&ui, &poll_state, poll_locale.get());
    });

    let elapsed_state = Rc::clone(&state);
    let elapsed_ui = ui.as_weak();
    let elapsed_locale = Rc::clone(&locale);
    let elapsed_timer = slint::Timer::default();
    elapsed_timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
        let state = elapsed_state.borrow();
        let Some(started_at) = state.started_at else { return };
        let Some(ui) = elapsed_ui.upgrade() else { return };
        let elapsed = started_at.elapsed().as_secs();
        let timeout = DEFAULT_METADATA_TIMEOUT.as_secs();
        let remaining = timeout.saturating_sub(elapsed);
        let template = I18nCatalog::text(elapsed_locale.get(), TextKey::SearchSearchingTemplate);
        ui.set_search_status(
            template
                .replace("{elapsed}", &elapsed.to_string())
                .replace("{remaining}", &remaining.to_string())
                .into(),
        );
    });

    state.borrow_mut().timers.extend([poll_timer, elapsed_timer]);
}

fn install_url_edit(ui: &AppWindow, state: Rc<RefCell<SearchState>>) {
    let ui_weak = ui.as_weak();
    ui.on_search_url_edited(move |_| {
        if let Some(ui) = ui_weak.upgrade() {
            update_can_download(&ui, &state);
        }
    });
}

fn install_path_edit(ui: &AppWindow, state: Rc<RefCell<SearchState>>, locale: Rc<Cell<Locale>>) {
    let ui_weak = ui.as_weak();
    let timer = Rc::new(RefCell::new(slint::Timer::default()));
    ui.on_search_download_path_edited(move |value| {
        let value = value.to_string();
        let timer = Rc::clone(&timer);
        let state = Rc::clone(&state);
        let locale = Rc::clone(&locale);
        let ui_weak = ui_weak.clone();
        timer
            .borrow_mut()
            .start(slint::TimerMode::SingleShot, Duration::from_millis(500), move || {
                let error = validate_download_path(&value).err();
                state.borrow_mut().path_error = error;
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_search_path_error(path_error_text(locale.get(), error).into());
                    update_can_download(&ui, &state);
                }
            });
    });
}

fn install_browse(ui: &AppWindow, state: Rc<RefCell<SearchState>>, locale: Rc<Cell<Locale>>) {
    let ui_weak = ui.as_weak();
    ui.on_search_browse_download_requested(move || {
        let Some(path) = crate::app::configure::picker::choose_directory() else {
            return;
        };
        let value = path.display().to_string();
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_search_download_path(value.clone().into());
            state.borrow_mut().path_error = validate_download_path(&value).err();
            ui.set_search_path_error(path_error_text(locale.get(), state.borrow().path_error).into());
            update_can_download(&ui, &state);
        }
    });
}

fn install_default_path(
    ui: &AppWindow,
    storage: &'static Storage,
    state: Rc<RefCell<SearchState>>,
    locale: Rc<Cell<Locale>>,
) {
    let ui_weak = ui.as_weak();
    ui.on_search_use_default_download_requested(move || {
        let Ok(Some(path)) = storage.default_download_path() else {
            return;
        };
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_search_download_path(path.clone().into());
            state.borrow_mut().path_error = validate_download_path(&path).err();
            ui.set_search_path_error(path_error_text(locale.get(), state.borrow().path_error).into());
            update_can_download(&ui, &state);
        }
    });
}

fn install_search(
    ui: &AppWindow,
    storage: &'static Storage,
    state: Rc<RefCell<SearchState>>,
    locale: Rc<Cell<Locale>>,
) {
    let ui_weak = ui.as_weak();
    ui.on_search_requested(move || {
        if state.borrow().cancelled.is_some() {
            return;
        }
        let Some(ui) = ui_weak.upgrade() else { return };
        let url = ui.get_search_url().to_string();
        if url.trim().is_empty() {
            return;
        }
        let configuration = match storage.configuration() {
            Ok(Some(configuration)) => configuration,
            _ => {
                set_failure(&ui, &state, locale.get(), SearchFailure::ConfigurationMissing);
                return;
            }
        };
        if configuration.yt_dlp_path.trim().is_empty() {
            set_failure(&ui, &state, locale.get(), SearchFailure::YtDlpPathMissing);
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let client = DownloadTaskClient::new(
            configuration.yt_dlp_path,
            Some(configuration.proxy),
            DEFAULT_METADATA_TIMEOUT,
            ui.get_search_download_path().to_string(),
        );
        let callback_sender = sender.clone();
        let handle = match client.inspect_url(url, move |message| {
            let _ = callback_sender.send(SearchEvent::Message(message));
        }) {
            Ok(handle) => handle,
            Err(error) => {
                set_failure(&ui, &state, locale.get(), classify_failure(&error));
                return;
            }
        };
        let cancelled = handle.cancellation_token();
        thread::spawn(move || {
            let result = handle.wait();
            let _ = sender.send(SearchEvent::Completion(result));
        });
        state.borrow_mut().cancelled = Some(cancelled);
        state.borrow_mut().receiver = Some(receiver);
        {
            let mut state = state.borrow_mut();
            state.metadata = None;
            state.selected_index = None;
            state.selected_original_index = None;
            state.sort_column = None;
            state.sort_direction = SortDirection::Reset;
            state.visible_order.clear();
            state.started_at = Some(Instant::now());
        }
        ui.set_search_sort_column(-1);
        ui.set_search_sort_direction(SortDirection::Reset.index());
        ui.set_search_video_title("".into());
        ui.set_search_busy(true);
        ui.set_search_status_kind(0);
        ui.set_search_status(
            I18nCatalog::text(locale.get(), TextKey::SearchSearchingTemplate)
                .replace("{elapsed}", "0")
                .replace("{remaining}", &DEFAULT_METADATA_TIMEOUT.as_secs().to_string())
                .into(),
        );
        ui.set_search_results(ModelRc::new(VecModel::from(Vec::<SearchResultRow>::new())));
        ui.set_search_selected_index(-1);
        update_can_download(&ui, &state);
    });
}

fn install_stop(ui: &AppWindow, state: Rc<RefCell<SearchState>>) {
    let ui_weak = ui.as_weak();
    ui.on_search_stop_requested(move || {
        if let Some(cancelled) = state.borrow_mut().cancelled.take() {
            cancelled.store(true, Ordering::Release);
        }
        state.borrow_mut().receiver = None;
        state.borrow_mut().started_at = None;
        state.borrow_mut().metadata = None;
        {
            let mut state = state.borrow_mut();
            state.selected_index = None;
            state.selected_original_index = None;
            state.sort_column = None;
            state.sort_direction = SortDirection::Reset;
            state.visible_order.clear();
        }
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_search_busy(false);
            ui.set_search_status("".into());
            ui.set_search_video_title("".into());
            ui.set_search_status_kind(0);
            ui.set_search_results(ModelRc::new(VecModel::from(Vec::<SearchResultRow>::new())));
            ui.set_search_selected_index(-1);
            ui.set_search_sort_column(-1);
            ui.set_search_sort_direction(SortDirection::Reset.index());
            update_can_download(&ui, &state);
        }
    });
}

fn install_result_selection(ui: &AppWindow, state: Rc<RefCell<SearchState>>) {
    let ui_weak = ui.as_weak();
    ui.on_search_result_selected(move |index| {
        let Ok(index) = usize::try_from(index) else { return };
        let Some(original_index) = state.borrow().visible_order.get(index).copied() else {
            return;
        };
        {
            let mut state = state.borrow_mut();
            state.selected_index = Some(index);
            state.selected_original_index = Some(original_index);
        }
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_search_selected_index(index as i32);
            update_can_download(&ui, &state);
        }
    });
}

fn install_result_sort(ui: &AppWindow, state: Rc<RefCell<SearchState>>) {
    let ui_weak = ui.as_weak();
    ui.on_search_result_sort_requested(move |column| {
        let Some(column) = SortColumn::from_index(column) else {
            return;
        };
        let Some(ui) = ui_weak.upgrade() else { return };
        if ui.get_search_busy() {
            return;
        }
        let (next_column, next_direction, selected_original_index, metadata) = {
            let state = state.borrow();
            let (next_column, next_direction) = next_sort_state(state.sort_column, state.sort_direction, column);
            (
                next_column,
                next_direction,
                state.selected_original_index,
                state.metadata.clone(),
            )
        };
        let Some(video) = metadata else { return };
        let visible_order = sorted_result_indices(&video, next_column, next_direction);
        let rows = result_rows_in_order(&video, &visible_order)
            .into_iter()
            .map(|row| SearchResultRow {
                format_id: row.format_id.into(),
                format_note: row.format_note.into(),
                extension: row.extension.into(),
                resolution: row.resolution.into(),
                bitrate: row.bitrate.into(),
                file_size: row.file_size.into(),
                video_codec: row.video_codec.into(),
                audio_codec: row.audio_codec.into(),
            })
            .collect::<Vec<_>>();
        let selected_index =
            selected_original_index.and_then(|selected| visible_order.iter().position(|&index| index == selected));
        {
            let mut state = state.borrow_mut();
            state.sort_column = next_column;
            state.sort_direction = next_direction;
            state.visible_order = visible_order;
            state.selected_index = selected_index;
        }
        ui.set_search_results(ModelRc::new(VecModel::from(rows)));
        ui.set_search_sort_column(next_column.map_or(-1, SortColumn::index));
        ui.set_search_sort_direction(next_direction.index());
        ui.set_search_selected_index(selected_index.map_or(-1, |index| index as i32));
        update_can_download(&ui, &state);
    });
}

fn poll_events(ui: &AppWindow, state: &Rc<RefCell<SearchState>>, locale: Locale) {
    let mut terminal = None;
    let mut metadata = None;
    let receiver = state.borrow_mut().receiver.take();
    let Some(receiver) = receiver else { return };
    loop {
        match receiver.try_recv() {
            Ok(SearchEvent::Message(MediaMessage::Metadata(video))) => metadata = Some(video),
            Ok(SearchEvent::Message(MediaMessage::Cancelled)) => terminal = Some(Err(SearchFailure::Cancelled)),
            Ok(SearchEvent::Message(MediaMessage::TimedOut)) => terminal = Some(Err(SearchFailure::TimedOut)),
            Ok(SearchEvent::Message(MediaMessage::Finished)) => terminal = Some(Ok(())),
            Ok(SearchEvent::Message(MediaMessage::Started)) => {}
            Ok(SearchEvent::Completion(Ok(video))) => metadata = Some(video),
            Ok(SearchEvent::Completion(Err(error))) => terminal = Some(Err(classify_failure(&error))),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
    let mut current = state.borrow_mut();
    if let Some(video) = metadata {
        current.metadata = Some(video);
    }
    if terminal.is_none() {
        current.receiver = Some(receiver);
        return;
    }
    current.started_at = None;
    current.cancelled = None;
    current.receiver = None;
    let result = terminal.unwrap();
    drop(current);
    match result {
        Ok(()) => {
            let video = state.borrow().metadata.clone();
            if let Some(video) = video {
                let visible_order = (0..video.formats.len()).collect::<Vec<_>>();
                let rows = result_rows_in_order(&video, &visible_order)
                    .into_iter()
                    .map(|row| SearchResultRow {
                        format_id: row.format_id.into(),
                        format_note: row.format_note.into(),
                        extension: row.extension.into(),
                        resolution: row.resolution.into(),
                        bitrate: row.bitrate.into(),
                        file_size: row.file_size.into(),
                        video_codec: row.video_codec.into(),
                        audio_codec: row.audio_codec.into(),
                    })
                    .collect::<Vec<_>>();
                state.borrow_mut().visible_order = visible_order;
                ui.set_search_results(ModelRc::new(VecModel::from(rows)));
            }
            ui.set_search_sort_column(-1);
            ui.set_search_sort_direction(SortDirection::Reset.index());
            ui.set_search_selected_index(-1);
            if let Some(video) = state.borrow().metadata.as_ref() {
                ui.set_search_video_title(video.title.clone().into());
            }
            ui.set_search_busy(false);
            ui.set_search_status_kind(1);
            ui.set_search_status(I18nCatalog::text(locale, TextKey::SearchSuccess).into());
        }
        Err(failure) => set_failure(ui, state, locale, failure),
    }
    update_can_download(ui, state);
}

fn set_failure(ui: &AppWindow, state: &Rc<RefCell<SearchState>>, locale: Locale, failure: SearchFailure) {
    {
        let mut state = state.borrow_mut();
        state.started_at = None;
        state.cancelled = None;
        state.receiver = None;
        state.metadata = None;
        state.visible_order.clear();
        state.selected_index = None;
        state.selected_original_index = None;
        state.sort_column = None;
        state.sort_direction = SortDirection::Reset;
    }
    ui.set_search_sort_column(-1);
    ui.set_search_sort_direction(SortDirection::Reset.index());
    ui.set_search_selected_index(-1);
    ui.set_search_video_title("".into());
    ui.set_search_results(ModelRc::new(VecModel::from(Vec::<SearchResultRow>::new())));
    ui.set_search_busy(false);
    ui.set_search_status_kind(2);
    ui.set_search_status(failure_text(locale, failure).into());
    update_can_download(ui, state);
}

fn update_can_download(ui: &AppWindow, state: &Rc<RefCell<SearchState>>) {
    let path = ui.get_search_download_path();
    let enabled = can_download(&path, state.borrow().selected_index, state.borrow().path_error);
    ui.set_search_can_download(enabled);
    ui.set_search_can_search(!ui.get_search_url().trim().is_empty() && !ui.get_search_busy());
}

fn path_error_text(locale: Locale, error: Option<SearchPathError>) -> &'static str {
    match error {
        None => "",
        Some(SearchPathError::LeadingOrTrailingWhitespace) => {
            I18nCatalog::text(locale, TextKey::SearchErrorPathWhitespace)
        }
        Some(SearchPathError::MissingDirectory) => I18nCatalog::text(locale, TextKey::SearchErrorPathMissing),
        Some(SearchPathError::NotADirectory) => I18nCatalog::text(locale, TextKey::SearchErrorPathFile),
    }
}

fn failure_text(locale: Locale, failure: SearchFailure) -> &'static str {
    match failure {
        SearchFailure::ConfigurationMissing => I18nCatalog::text(locale, TextKey::SearchErrorConfig),
        SearchFailure::YtDlpPathMissing => I18nCatalog::text(locale, TextKey::SearchErrorYtdlp),
        SearchFailure::Process => I18nCatalog::text(locale, TextKey::SearchErrorProcess),
        SearchFailure::Metadata => I18nCatalog::text(locale, TextKey::SearchErrorMetadata),
        SearchFailure::TimedOut => I18nCatalog::text(locale, TextKey::SearchTimeout),
        SearchFailure::Cancelled => I18nCatalog::text(locale, TextKey::SearchCancelled),
        SearchFailure::InvalidPath | SearchFailure::Unexpected => {
            I18nCatalog::text(locale, TextKey::SearchErrorUnexpected)
        }
    }
}
