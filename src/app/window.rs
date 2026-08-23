use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;

use super::{AppWindow, EffectiveTheme, I18n, TextScale, Theme, ThemeMode};

const PROJECT_URL: &str = "https://github.com/Kylineyes/yt-dlp-gui";

// 主题和 i18n 在窗口进入事件循环前初始化，避免首帧显示未解析状态。
use crate::app::dialog::{DialogButtons, DialogRequest, DialogService, DialogTitle, DialogVisualState};
use crate::app::navigation::NavigationState;
use crate::design_system::i18n::{I18nCatalog, I18nSnapshot, Locale};
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

    crate::app::configure_window::install(
        &ui,
        storage,
        configuration.clone(),
        Rc::clone(&mode_state),
        Rc::clone(&locale_state),
        apply_theme,
        set_i18n,
    );

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
