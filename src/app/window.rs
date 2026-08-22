slint::include_modules!();

use std::time::Duration;

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
        .map(|configuration| theme_mode(configuration.theme))
        .unwrap_or(RustThemeMode::DEFAULT);
    let locale = configuration
        .as_ref()
        .map(|configuration| Locale::parse(&configuration.language))
        .unwrap_or(Locale::DEFAULT);
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

    let theme_timer = if mode == RustThemeMode::System {
        // 轮询 Windows 主题设置，更新令牌而不重建窗口或丢失页面状态。
        let ui_weak = ui.as_weak();
        let timer = slint::Timer::default();
        timer.start(slint::TimerMode::Repeated, Duration::from_secs(1), move || {
            if let Some(ui) = ui_weak.upgrade() {
                let effective = mode.resolve(system_theme(), dark_theme_available());
                ui.global::<Theme>()
                    .set_effective_theme(slint_effective_theme(effective));
            }
        });
        Some(timer)
    } else {
        None
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

    ui.on_project_url_requested(|| {
        let _ = webbrowser::open(PROJECT_URL);
    });

    let result = ui.run();
    drop(theme_timer);
    result
}

fn theme_mode(value: i8) -> RustThemeMode {
    match value {
        1 => RustThemeMode::Light,
        2 => RustThemeMode::Dark,
        _ => RustThemeMode::DEFAULT,
    }
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
}
