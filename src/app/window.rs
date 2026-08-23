slint::include_modules!();

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

const PROJECT_URL: &str = "https://github.com/Kylineyes/yt-dlp-gui";

// 主题和 i18n 在窗口进入事件循环前初始化，避免首帧显示未解析状态。
use crate::app::configure::{
    find_on_path, normalize_draft, validate, ConfigureError, ConfigureField, ConfigureValidationError,
};
use crate::app::dialog::{DialogButtons, DialogRequest, DialogService, DialogTitle, DialogVisualState};
use crate::app::navigation::NavigationState;
use crate::design_system::i18n::{I18nCatalog, I18nSnapshot, Locale, TextKey};
use crate::design_system::theme::{
    dark_theme_available, system_theme, EffectiveTheme as RustEffectiveTheme, TextScale as RustTextScale,
    ThemeMode as RustThemeMode,
};
use crate::storage::StorageError;

pub fn show_storage_error(error: StorageError) -> Result<(), Box<dyn std::error::Error>> {
    let mode = RustThemeMode::DEFAULT;
    let effective_theme = mode.resolve(system_theme(), dark_theme_available());
    let description = format!("{}\n\n请检查配置数据库路径、文件权限和 SQLite 错误详情。", error);
    let request = DialogRequest {
        title: "配置加载失败",
        description: &description,
        confirm_label: "确认",
        cancel_label: "",
        title_kind: DialogTitle::Error,
        buttons: DialogButtons::ConfirmOnly,
    };
    let _dialog = DialogService::show(
        request,
        None,
        DialogVisualState {
            effective_theme,
            text_scale: RustTextScale::Default,
        },
        |_| {
            slint::quit_event_loop().ok();
        },
    )?;
    slint::run_event_loop()?;
    Ok(())
}

pub fn run() -> Result<(), slint::PlatformError> {
    let storage = crate::storage::Storage::instance().expect("存储模块必须先于主窗口初始化");
    let configuration = storage.configuration().expect("启动后存储配置必须可以同步读取");
    let ui = AppWindow::new()?;
    let mut navigation = NavigationState::new();
    let mode = configuration
        .as_ref()
        .map(|configuration| RustThemeMode::parse(&configuration.theme))
        .unwrap_or(RustThemeMode::DEFAULT);
    let mode_state = Rc::new(RefCell::new(mode));
    // draft 只承载页面未保存的编辑态，只有保存成功后才会写入 Storage。
    let draft = Rc::new(RefCell::new(
        configuration
            .clone()
            .unwrap_or_else(crate::storage::EnvironmentConfig::draft_default),
    ));
    let locale = configuration
        .as_ref()
        .map(|configuration| Locale::parse(&configuration.language))
        .unwrap_or(Locale::DEFAULT);
    let locale_state = Rc::new(Cell::new(locale));
    let effective = mode.resolve(system_theme(), dark_theme_available());

    {
        let theme = ui.global::<Theme>();
        theme.set_mode(slint_theme_mode(mode));
        theme.set_effective_theme(slint_effective_theme(effective));
        theme.set_text_scale(slint_text_scale(RustTextScale::Default));
    }

    {
        let i18n = ui.global::<I18n>();
        set_i18n(&i18n, locale);
    }

    let theme_timer = {
        // System 模式只轮询有效主题；用户选择和配置草稿本身不会被改写。
        let ui_weak = ui.as_weak();
        let mode_state = Rc::clone(&mode_state);
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            if let Some(ui) = ui_weak.upgrade() {
                let mode = *mode_state.borrow();
                if mode == RustThemeMode::System {
                    let effective = mode.resolve(system_theme(), dark_theme_available());
                    ui.global::<Theme>()
                        .set_effective_theme(slint_effective_theme(effective));
                }
            }
        });
        Some(timer)
    };

    ui.set_current_route(navigation.current().index());
    // Slint 只发出目标索引，Rust 侧用 NavigationState 负责合法性校验。
    let ui_weak = ui.as_weak();
    ui.on_route_requested(move |index| {
        if navigation.navigate_to_index(index) {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_current_route(navigation.current().index());
            }
        }
    });

    {
        let draft = Rc::clone(&draft);
        ui.set_configure_yt_dlp_path(draft.borrow().yt_dlp_path.clone().into());
        ui.set_configure_ffmpeg_path(draft.borrow().ffmpeg_path.clone().into());
        ui.set_configure_download_path(draft.borrow().default_download_path.clone().into());
        ui.set_configure_proxy(draft.borrow().proxy.clone().into());
        let draft = draft.borrow();
        ui.set_configure_concurrent_downloads(draft.concurrent_downloads.to_string().into());
        ui.set_configure_concurrent_index(draft.concurrent_downloads as i32);
        ui.set_configure_language(draft.language.clone().into());
        ui.set_configure_language_index(if draft.language == "zh-CN" { 1 } else { 0 });
        ui.set_configure_theme(draft.theme.clone().into());
        ui.set_configure_theme_index(match draft.theme.as_str() {
            "light" => 1,
            "dark" => 2,
            _ => 0,
        });
    }

    // 防抖只保留最后一次编辑结果，避免每次按键都访问文件系统。
    let validation_timer = Rc::new(RefCell::new(slint::Timer::default()));
    // 两层保护分别覆盖 Rust 回调和 Slint 双向绑定的回流。
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
                        let saved_locale = Locale::parse(&configuration.language);
                        locale_state.set(saved_locale);
                        set_i18n(&ui.global::<I18n>(), saved_locale);
                        ui.set_configure_status(
                            I18nCatalog::text(
                                Locale::parse(&configuration.language),
                                crate::design_system::i18n::TextKey::ConfigureSaved,
                            )
                            .into(),
                        );
                    }
                }
                Err(_) => {
                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_configure_status(
                            I18nCatalog::text(
                                Locale::parse(&configuration.language),
                                crate::design_system::i18n::TextKey::ConfigureStorageError,
                            )
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
            // 批量更新下拉和输入值时禁止回调，否则绑定回流会覆盖默认草稿。
            reset_guard.set(true);
            ui_weak.upgrade().map(|ui| ui.set_configure_suppress_callbacks(true));
            let configuration = crate::storage::EnvironmentConfig::draft_default();
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
                ui.set_configure_language(configuration.language.clone().into());
                ui.set_configure_language_index(0);
                ui.set_configure_theme(configuration.theme.clone().into());
                ui.set_configure_theme_index(0);
                let reset_mode = RustThemeMode::parse(&configuration.theme);
                let reset_locale = Locale::parse(&configuration.language);
                apply_theme(&ui, reset_mode);
                set_i18n(&ui.global::<I18n>(), reset_locale);
                clear_validation_errors(&ui);
                ui.set_configure_status("".into());
            }
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_configure_suppress_callbacks(false);
            }
            reset_guard.set(false);
        });
    }

    {
        let draft = Rc::clone(&draft);
        let validation_timer = Rc::clone(&validation_timer);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        let ui_weak = ui.as_weak();
        ui.on_configure_auto_find_ytdlp_requested(move || {
            if let Some(path) = find_on_path("yt-dlp.exe") {
                draft.borrow_mut().yt_dlp_path = path.display().to_string();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_configure_yt_dlp_path(path.display().to_string().into());
                }
                schedule_validation(
                    &validation_timer,
                    Rc::clone(&draft),
                    ui_weak.clone(),
                    Rc::clone(&last_error),
                    Rc::clone(&locale_state),
                );
            }
        });
    }
    {
        let draft = Rc::clone(&draft);
        let validation_timer = Rc::clone(&validation_timer);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        let ui_weak = ui.as_weak();
        ui.on_configure_auto_find_ffmpeg_requested(move || {
            if let Some(path) = find_on_path("ffmpeg.exe") {
                draft.borrow_mut().ffmpeg_path = path.display().to_string();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_configure_ffmpeg_path(path.display().to_string().into());
                }
                schedule_validation(
                    &validation_timer,
                    Rc::clone(&draft),
                    ui_weak.clone(),
                    Rc::clone(&last_error),
                    Rc::clone(&locale_state),
                );
            }
        });
    }
    {
        let draft = Rc::clone(&draft);
        let validation_timer = Rc::clone(&validation_timer);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        let ui_weak = ui.as_weak();
        ui.on_configure_browse_ytdlp_requested(move || {
            if let Some(path) = crate::app::configure::picker::choose_executable() {
                let value = path.display().to_string();
                draft.borrow_mut().yt_dlp_path = value.clone();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_configure_yt_dlp_path(value.into());
                }
                schedule_validation(
                    &validation_timer,
                    Rc::clone(&draft),
                    ui_weak.clone(),
                    Rc::clone(&last_error),
                    Rc::clone(&locale_state),
                );
            }
        });
    }
    {
        let draft = Rc::clone(&draft);
        let validation_timer = Rc::clone(&validation_timer);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        let ui_weak = ui.as_weak();
        ui.on_configure_browse_ffmpeg_requested(move || {
            if let Some(path) = crate::app::configure::picker::choose_executable() {
                let value = path.display().to_string();
                draft.borrow_mut().ffmpeg_path = value.clone();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_configure_ffmpeg_path(value.into());
                }
                schedule_validation(
                    &validation_timer,
                    Rc::clone(&draft),
                    ui_weak.clone(),
                    Rc::clone(&last_error),
                    Rc::clone(&locale_state),
                );
            }
        });
    }
    {
        let draft = Rc::clone(&draft);
        let validation_timer = Rc::clone(&validation_timer);
        let locale_state = Rc::clone(&locale_state);
        let last_error = Rc::clone(&last_error);
        let ui_weak = ui.as_weak();
        ui.on_configure_browse_download_requested(move || {
            if let Some(path) = crate::app::configure::picker::choose_directory() {
                let value = path.display().to_string();
                draft.borrow_mut().default_download_path = value.clone();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_configure_download_path(value.into());
                }
                schedule_validation(
                    &validation_timer,
                    Rc::clone(&draft),
                    ui_weak.clone(),
                    Rc::clone(&last_error),
                    Rc::clone(&locale_state),
                );
            }
        });
    }

    ui.on_project_url_requested(|| {
        let _ = webbrowser::open(PROJECT_URL);
    });

    let result = ui.run();
    drop(theme_timer);
    result
}

fn apply_theme(ui: &AppWindow, mode: RustThemeMode) {
    let effective = mode.resolve(system_theme(), dark_theme_available());
    let theme = ui.global::<Theme>();
    theme.set_mode(slint_theme_mode(mode));
    theme.set_effective_theme(slint_effective_theme(effective));
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
        ConfigureField::Language | ConfigureField::Theme => ui.set_configure_option_error(message),
    }
}

// 文件和目录校验可能阻塞 UI，因此只在用户停止编辑 500ms 后执行一次。
fn schedule_validation(
    timer: &Rc<RefCell<slint::Timer>>,
    draft: Rc<RefCell<crate::storage::EnvironmentConfig>>,
    ui_weak: slint::Weak<AppWindow>,
    last_error: Rc<RefCell<Option<ConfigureValidationError>>>,
    locale_state: Rc<Cell<Locale>>,
) {
    timer
        .borrow_mut()
        .start(slint::TimerMode::SingleShot, Duration::from_millis(500), move || {
            let error = validate(&draft.borrow()).err();
            *last_error.borrow_mut() = error.clone();
            if let Some(ui) = ui_weak.upgrade() {
                set_validation_error(&ui, error.as_ref(), locale_state.get());
            }
        });
}

fn clear_validation_errors(ui: &AppWindow) {
    ui.set_configure_yt_dlp_error("".into());
    ui.set_configure_ffmpeg_error("".into());
    ui.set_configure_download_error("".into());
    ui.set_configure_proxy_error("".into());
    ui.set_configure_concurrent_error("".into());
    ui.set_configure_option_error("".into());
}

fn slint_theme_mode(mode: RustThemeMode) -> ThemeMode {
    // Rust 领域类型与 Slint 生成枚举同名，因此这里显式转换。
    match mode {
        RustThemeMode::System => ThemeMode::System,
        RustThemeMode::Light => ThemeMode::Light,
        RustThemeMode::Dark => ThemeMode::Dark,
    }
}

fn slint_effective_theme(theme: RustEffectiveTheme) -> EffectiveTheme {
    match theme {
        RustEffectiveTheme::Light => EffectiveTheme::Light,
        RustEffectiveTheme::Dark => EffectiveTheme::Dark,
    }
}

fn slint_text_scale(scale: RustTextScale) -> TextScale {
    match scale {
        RustTextScale::Default => TextScale::Default,
        RustTextScale::Large => TextScale::Large,
        RustTextScale::ExtraLarge => TextScale::ExtraLarge,
    }
}

fn set_i18n(i18n: &I18n<'_>, locale: Locale) {
    let snapshot = I18nCatalog::snapshot(locale);
    i18n.set_locale(locale.as_str().into());
    apply_i18n_snapshot(i18n, snapshot);
}

fn apply_i18n_snapshot(i18n: &I18n<'_>, snapshot: I18nSnapshot) {
    i18n.set_app_title(snapshot.app_title.into());
    i18n.set_nav_welcome(snapshot.nav_welcome.into());
    i18n.set_nav_configure(snapshot.nav_configure.into());
    i18n.set_nav_search(snapshot.nav_search.into());
    i18n.set_nav_tasks(snapshot.nav_tasks.into());
    i18n.set_welcome_title(snapshot.welcome_title.into());
    i18n.set_welcome_introduction(snapshot.welcome_introduction.into());
    i18n.set_welcome_step_configure_number(snapshot.welcome_step_configure_number.into());
    i18n.set_welcome_step_configure_description(snapshot.welcome_step_configure_description.into());
    i18n.set_welcome_step_configure_page(snapshot.welcome_step_configure_page.into());
    i18n.set_welcome_step_search_number(snapshot.welcome_step_search_number.into());
    i18n.set_welcome_step_search_description(snapshot.welcome_step_search_description.into());
    i18n.set_welcome_step_search_page(snapshot.welcome_step_search_page.into());
    i18n.set_welcome_step_tasks_number(snapshot.welcome_step_tasks_number.into());
    i18n.set_welcome_step_tasks_description(snapshot.welcome_step_tasks_description.into());
    i18n.set_welcome_step_tasks_page(snapshot.welcome_step_tasks_page.into());
    i18n.set_welcome_dependencies_title(snapshot.welcome_dependencies_title.into());
    i18n.set_welcome_dependency_slint(snapshot.welcome_dependency_slint.into());
    i18n.set_welcome_dependency_webbrowser(snapshot.welcome_dependency_webbrowser.into());
    i18n.set_welcome_dependency_windows_sys(snapshot.welcome_dependency_windows_sys.into());
    i18n.set_welcome_dependency_slint_build(snapshot.welcome_dependency_slint_build.into());
    i18n.set_welcome_thanks(snapshot.welcome_thanks.into());
    i18n.set_welcome_project_label(snapshot.welcome_project_label.into());
    i18n.set_welcome_project_url(snapshot.welcome_project_url.into());
    i18n.set_configure_title(snapshot.configure_title.into());
    i18n.set_configure_introduction(snapshot.configure_introduction.into());
    i18n.set_configure_ytdlp_path_label(snapshot.configure_ytdlp_path_label.into());
    i18n.set_configure_ytdlp_path_placeholder(snapshot.configure_ytdlp_path_placeholder.into());
    i18n.set_configure_browser_label(snapshot.configure_browser_label.into());
    i18n.set_configure_language_label(snapshot.configure_language_label.into());
    i18n.set_configure_theme_label(snapshot.configure_theme_label.into());
    i18n.set_configure_theme_system(snapshot.configure_theme_system.into());
    i18n.set_configure_theme_light(snapshot.configure_theme_light.into());
    i18n.set_configure_theme_dark(snapshot.configure_theme_dark.into());
    i18n.set_configure_save(snapshot.configure_save.into());
    i18n.set_configure_reset(snapshot.configure_reset.into());
    i18n.set_configure_loading(snapshot.configure_loading.into());
    i18n.set_configure_saving(snapshot.configure_saving.into());
    i18n.set_configure_saved(snapshot.configure_saved.into());
    i18n.set_configure_validation_error(snapshot.configure_validation_error.into());
    i18n.set_configure_storage_error(snapshot.configure_storage_error.into());
    i18n.set_configure_program_settings(snapshot.configure_program_settings.into());
    i18n.set_configure_download_settings(snapshot.configure_download_settings.into());
    i18n.set_configure_third_party(snapshot.configure_third_party.into());
    i18n.set_configure_ffmpeg_path_label(snapshot.configure_ffmpeg_path_label.into());
    i18n.set_configure_ffmpeg_path_placeholder(snapshot.configure_ffmpeg_path_placeholder.into());
    i18n.set_configure_download_path_label(snapshot.configure_download_path_label.into());
    i18n.set_configure_download_path_placeholder(snapshot.configure_download_path_placeholder.into());
    i18n.set_configure_proxy_label(snapshot.configure_proxy_label.into());
    i18n.set_configure_proxy_placeholder(snapshot.configure_proxy_placeholder.into());
    i18n.set_configure_concurrent_label(snapshot.configure_concurrent_label.into());
    i18n.set_configure_concurrent_placeholder(snapshot.configure_concurrent_placeholder.into());
    i18n.set_configure_language_english(snapshot.configure_language_english.into());
    i18n.set_configure_language_chinese(snapshot.configure_language_chinese.into());
    i18n.set_configure_browse_file(snapshot.configure_browse_file.into());
    i18n.set_configure_browse_folder(snapshot.configure_browse_folder.into());
    i18n.set_configure_auto_find(snapshot.configure_auto_find.into());
    i18n.set_configure_concurrent_help(snapshot.configure_concurrent_help.into());
    i18n.set_configure_error_required(snapshot.configure_error_required.into());
    i18n.set_configure_error_whitespace(snapshot.configure_error_whitespace.into());
    i18n.set_configure_error_missing_file(snapshot.configure_error_missing_file.into());
    i18n.set_configure_error_not_file(snapshot.configure_error_not_file.into());
    i18n.set_configure_error_missing_directory(snapshot.configure_error_missing_directory.into());
    i18n.set_configure_error_not_directory(snapshot.configure_error_not_directory.into());
    i18n.set_configure_error_invalid_number(snapshot.configure_error_invalid_number.into());
    i18n.set_configure_error_invalid_option(snapshot.configure_error_invalid_option.into());
    i18n.set_configure_error_invalid_tool_name(snapshot.configure_error_invalid_tool_name.into());
    i18n.set_configure_error_invalid_tool_extension(snapshot.configure_error_invalid_tool_extension.into());
    i18n.set_configure_tool_not_found(snapshot.configure_tool_not_found.into());
    i18n.set_configure_picker_cancelled(snapshot.configure_picker_cancelled.into());
    i18n.set_configure_picker_failed(snapshot.configure_picker_failed.into());
    i18n.set_configure_searching(snapshot.configure_searching.into());
}
