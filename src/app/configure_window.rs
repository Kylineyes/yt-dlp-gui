use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;

use super::{AppWindow, I18n};
use crate::app::configure::{
    find_on_path, normalize_draft, validate, ConfigureError, ConfigureField, ConfigureValidationError,
};
use crate::design_system::i18n::{I18nCatalog, Locale, TextKey};
use crate::design_system::theme::ThemeMode as RustThemeMode;
use crate::storage::{EnvironmentConfig, Storage};

pub(super) fn install(
    ui: &AppWindow,
    storage: &'static Storage,
    initial: Option<EnvironmentConfig>,
    mode_state: Rc<RefCell<RustThemeMode>>,
    locale_state: Rc<Cell<Locale>>,
    apply_theme: fn(&AppWindow, RustThemeMode),
    set_i18n: for<'a> fn(&I18n<'a>, Locale),
) {
    let draft = Rc::new(RefCell::new(initial.unwrap_or_else(EnvironmentConfig::draft_default)));
    {
        let draft = draft.borrow();
        ui.set_configure_yt_dlp_path(draft.yt_dlp_path.clone().into());
        ui.set_configure_ffmpeg_path(draft.ffmpeg_path.clone().into());
        ui.set_configure_download_path(draft.default_download_path.clone().into());
        ui.set_configure_proxy(draft.proxy.clone().into());
        ui.set_configure_concurrent_downloads(draft.concurrent_downloads.to_string().into());
        ui.set_configure_concurrent_index(draft.concurrent_downloads as i32);
        ui.set_configure_search_timeout_sec(draft.search_timeout_sec.to_string().into());
        ui.set_configure_search_timeout_index(search_timeout_index(draft.search_timeout_sec));
        ui.set_configure_language(draft.language.clone().into());
        ui.set_configure_language_index(if draft.language == "zh-CN" { 1 } else { 0 });
        ui.set_configure_theme(draft.theme.clone().into());
        ui.set_configure_theme_index(match draft.theme.as_str() {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        });
    }

    let validation_timer = Rc::new(RefCell::new(slint::Timer::default()));
    let reset_guard = Rc::new(Cell::new(false));
    let last_error = Rc::new(RefCell::new(None::<ConfigureValidationError>));

    {
        let draft = Rc::clone(&draft);
        let validation_timer = Rc::clone(&validation_timer);
        let ui_weak = ui.as_weak();
        let mode_state = Rc::clone(&mode_state);
        let reset_guard = Rc::clone(&reset_guard);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        ui.on_configure_field_edited(move |field, value| {
            if reset_guard.get() {
                return;
            }
            let value = value.to_string();
            {
                let mut draft = draft.borrow_mut();
                match field.as_str() {
                    "yt-dlp-path" => draft.yt_dlp_path = value.clone(),
                    "ffmpeg-path" => draft.ffmpeg_path = value.clone(),
                    "download-path" => draft.default_download_path = value.clone(),
                    "proxy" => draft.proxy = value.clone(),
                    "concurrent-downloads" => draft.concurrent_downloads = value.parse().unwrap_or(-1),
                    "search-timeout-sec" => draft.search_timeout_sec = value.parse().unwrap_or(-1),
                    "language" => draft.language = value.clone(),
                    "theme" => draft.theme = value.clone(),
                    _ => return,
                }
            }
            if let Some(ui) = ui_weak.upgrade() {
                match field.as_str() {
                    "theme" => {
                        let mode = RustThemeMode::parse(&value);
                        *mode_state.borrow_mut() = mode;
                        apply_theme(&ui, mode);
                    }
                    "language" => {
                        let locale = Locale::parse(&value);
                        locale_state.set(locale);
                        set_i18n(&ui.global::<I18n>(), locale);
                        if let Some(error) = last_error.borrow().as_ref() {
                            set_validation_error(&ui, Some(error), locale);
                        }
                    }
                    _ => {}
                }
            }
            schedule_validation(
                &validation_timer,
                Rc::clone(&draft),
                ui_weak.clone(),
                Rc::clone(&last_error),
                Rc::clone(&locale_state),
            );
        });
    }

    {
        let draft = Rc::clone(&draft);
        let ui_weak = ui.as_weak();
        let mode_state = Rc::clone(&mode_state);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        ui.on_configure_save_requested(move || {
            let configuration = normalize_draft(draft.borrow().clone());
            if let Err(error) = validate(&configuration) {
                *last_error.borrow_mut() = Some(error.clone());
                if let Some(ui) = ui_weak.upgrade() {
                    set_validation_error(&ui, Some(&error), locale_state.get());
                }
                return;
            }
            *last_error.borrow_mut() = None;
            match storage.save_configuration(configuration.clone()) {
                Ok(()) => {
                    *draft.borrow_mut() = configuration.clone();
                    let mode = RustThemeMode::parse(&configuration.theme);
                    *mode_state.borrow_mut() = mode;
                    if let Some(ui) = ui_weak.upgrade() {
                        apply_theme(&ui, mode);
                        let locale = Locale::parse(&configuration.language);
                        locale_state.set(locale);
                        set_i18n(&ui.global::<I18n>(), locale);
                        ui.set_configure_status(I18nCatalog::text(locale, TextKey::ConfigureSaved).into());
                    }
                }
                Err(_) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_configure_status(
                            I18nCatalog::text(Locale::parse(&configuration.language), TextKey::ConfigureStorageError)
                                .into(),
                        );
                    }
                }
            }
        });
    }

    {
        let draft = Rc::clone(&draft);
        let ui_weak = ui.as_weak();
        let mode_state = Rc::clone(&mode_state);
        let locale_state = Rc::clone(&locale_state);
        let validation_timer = Rc::clone(&validation_timer);
        let reset_guard = Rc::clone(&reset_guard);
        let last_error = Rc::clone(&last_error);
        ui.on_configure_reset_requested(move || {
            reset_guard.set(true);
            ui_weak.upgrade().map(|ui| ui.set_configure_suppress_callbacks(true));
            let configuration = EnvironmentConfig::draft_default();
            *draft.borrow_mut() = configuration.clone();
            validation_timer.borrow_mut().stop();
            *last_error.borrow_mut() = None;
            *mode_state.borrow_mut() = RustThemeMode::parse(&configuration.theme);
            locale_state.set(Locale::parse(&configuration.language));
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_configure_yt_dlp_path(configuration.yt_dlp_path.into());
                ui.set_configure_ffmpeg_path(configuration.ffmpeg_path.into());
                ui.set_configure_download_path(configuration.default_download_path.into());
                ui.set_configure_proxy(configuration.proxy.into());
                ui.set_configure_concurrent_downloads(configuration.concurrent_downloads.to_string().into());
                ui.set_configure_concurrent_index(0);
                ui.set_configure_search_timeout_sec(configuration.search_timeout_sec.to_string().into());
                ui.set_configure_search_timeout_index(search_timeout_index(configuration.search_timeout_sec));
                ui.set_configure_language(configuration.language.clone().into());
                ui.set_configure_language_index(0);
                ui.set_configure_theme(configuration.theme.clone().into());
                ui.set_configure_theme_index(0);
                apply_theme(&ui, RustThemeMode::System);
                set_i18n(&ui.global::<I18n>(), Locale::EnUs);
                clear_validation_errors(&ui);
                ui.set_configure_status("".into());
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_configure_suppress_callbacks(false);
            }
            reset_guard.set(false);
        });
    }

    install_path_search(
        ui,
        &draft,
        &validation_timer,
        &locale_state,
        &last_error,
        "yt-dlp.exe",
        true,
    );
    install_path_search(
        ui,
        &draft,
        &validation_timer,
        &locale_state,
        &last_error,
        "ffmpeg.exe",
        false,
    );
    install_picker(ui, &draft, &validation_timer, &locale_state, &last_error, 0);
    install_picker(ui, &draft, &validation_timer, &locale_state, &last_error, 1);
    install_picker(ui, &draft, &validation_timer, &locale_state, &last_error, 2);
}

fn install_path_search(
    ui: &AppWindow,
    draft: &Rc<RefCell<EnvironmentConfig>>,
    timer: &Rc<RefCell<slint::Timer>>,
    locale: &Rc<Cell<Locale>>,
    error: &Rc<RefCell<Option<ConfigureValidationError>>>,
    executable: &'static str,
    yt_dlp: bool,
) {
    let draft = Rc::clone(draft);
    let timer = Rc::clone(timer);
    let locale = Rc::clone(locale);
    let error = Rc::clone(error);
    let ui_weak = ui.as_weak();
    let callback = move || {
        if let Some(path) = find_on_path(executable) {
            let value = path.display().to_string();
            if yt_dlp {
                draft.borrow_mut().yt_dlp_path = value.clone();
            } else {
                draft.borrow_mut().ffmpeg_path = value.clone();
            }
            if let Some(ui) = ui_weak.upgrade() {
                if yt_dlp {
                    ui.set_configure_yt_dlp_path(value.into());
                } else {
                    ui.set_configure_ffmpeg_path(value.into());
                }
            }
            schedule_validation(
                &timer,
                Rc::clone(&draft),
                ui_weak.clone(),
                error.clone(),
                locale.clone(),
            );
        }
    };
    if yt_dlp {
        ui.on_configure_auto_find_ytdlp_requested(callback);
    } else {
        ui.on_configure_auto_find_ffmpeg_requested(callback);
    }
}

fn install_picker(
    ui: &AppWindow,
    draft: &Rc<RefCell<EnvironmentConfig>>,
    timer: &Rc<RefCell<slint::Timer>>,
    locale: &Rc<Cell<Locale>>,
    error: &Rc<RefCell<Option<ConfigureValidationError>>>,
    kind: i32,
) {
    let draft = Rc::clone(draft);
    let timer = Rc::clone(timer);
    let locale = Rc::clone(locale);
    let error = Rc::clone(error);
    let ui_weak = ui.as_weak();
    let callback = move || {
        let path = if kind == 2 {
            crate::app::configure::picker::choose_directory()
        } else {
            crate::app::configure::picker::choose_executable()
        };
        if let Some(path) = path {
            let value = path.display().to_string();
            if kind == 0 {
                draft.borrow_mut().yt_dlp_path = value.clone();
            } else if kind == 1 {
                draft.borrow_mut().ffmpeg_path = value.clone();
            } else {
                draft.borrow_mut().default_download_path = value.clone();
            }
            if let Some(ui) = ui_weak.upgrade() {
                if kind == 0 {
                    ui.set_configure_yt_dlp_path(value.into());
                } else if kind == 1 {
                    ui.set_configure_ffmpeg_path(value.into());
                } else {
                    ui.set_configure_download_path(value.into());
                }
            }
            schedule_validation(
                &timer,
                Rc::clone(&draft),
                ui_weak.clone(),
                error.clone(),
                locale.clone(),
            );
        }
    };
    if kind == 0 {
        ui.on_configure_browse_ytdlp_requested(callback);
    } else if kind == 1 {
        ui.on_configure_browse_ffmpeg_requested(callback);
    } else {
        ui.on_configure_browse_download_requested(callback);
    }
}

fn set_validation_error(ui: &AppWindow, error: Option<&ConfigureValidationError>, locale: Locale) {
    clear_validation_errors(ui);
    let Some(error) = error else { return };
    let key = match error.error {
        ConfigureError::EmptyRequiredPath => TextKey::ConfigureErrorRequired,
        ConfigureError::HasLeadingOrTrailingWhitespace => TextKey::ConfigureErrorWhitespace,
        ConfigureError::MissingFile(_) => TextKey::ConfigureErrorMissingFile,
        ConfigureError::NotAFile(_) => TextKey::ConfigureErrorNotFile,
        ConfigureError::MissingDirectory => TextKey::ConfigureErrorMissingDirectory,
        ConfigureError::NotADirectory => TextKey::ConfigureErrorNotDirectory,
        ConfigureError::InvalidConcurrentDownloads => TextKey::ConfigureErrorInvalidNumber,
        ConfigureError::InvalidSearchTimeout => TextKey::ConfigureErrorInvalidOption,
        ConfigureError::InvalidLanguage | ConfigureError::InvalidTheme | ConfigureError::InvalidPath(_) => {
            TextKey::ConfigureErrorInvalidOption
        }
        ConfigureError::InvalidToolName(_) => TextKey::ConfigureErrorInvalidToolName,
        ConfigureError::InvalidToolExtension(_) => TextKey::ConfigureErrorInvalidToolExtension,
    };
    let message: slint::SharedString = I18nCatalog::text(locale, key).into();
    match error.field {
        ConfigureField::YtDlpPath => ui.set_configure_yt_dlp_error(message),
        ConfigureField::FfmpegPath => ui.set_configure_ffmpeg_error(message),
        ConfigureField::DefaultDownloadPath => ui.set_configure_download_error(message),
        ConfigureField::Proxy => ui.set_configure_proxy_error(message),
        ConfigureField::ConcurrentDownloads => ui.set_configure_concurrent_error(message),
        ConfigureField::SearchTimeout => ui.set_configure_search_timeout_error(message),
        ConfigureField::Language | ConfigureField::Theme => ui.set_configure_option_error(message),
    }
}

fn schedule_validation(
    timer: &Rc<RefCell<slint::Timer>>,
    draft: Rc<RefCell<EnvironmentConfig>>,
    ui_weak: slint::Weak<AppWindow>,
    last_error: Rc<RefCell<Option<ConfigureValidationError>>>,
    locale: Rc<Cell<Locale>>,
) {
    timer
        .borrow_mut()
        .start(slint::TimerMode::SingleShot, Duration::from_millis(500), move || {
            let error = validate(&draft.borrow()).err();
            *last_error.borrow_mut() = error.clone();
            if let Some(ui) = ui_weak.upgrade() {
                set_validation_error(&ui, error.as_ref(), locale.get());
            }
        });
}

fn clear_validation_errors(ui: &AppWindow) {
    ui.set_configure_yt_dlp_error("".into());
    ui.set_configure_ffmpeg_error("".into());
    ui.set_configure_download_error("".into());
    ui.set_configure_proxy_error("".into());
    ui.set_configure_concurrent_error("".into());
    ui.set_configure_search_timeout_error("".into());
    ui.set_configure_option_error("".into());
}

fn search_timeout_index(value: i64) -> i32 {
    match value {
        5 => 0,
        10 => 1,
        20 => 2,
        30 => 3,
        50 => 4,
        100 => 5,
        120 => 6,
        _ => 2,
    }
}
