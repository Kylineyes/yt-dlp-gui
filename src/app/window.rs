slint::include_modules!();

use std::time::Duration;

// 主题和 i18n 在窗口进入事件循环前初始化，避免首帧显示未解析状态。
use crate::app::navigation::NavigationState;
use crate::design_system::i18n::{I18nCatalog, Locale, TextKey};
use crate::design_system::theme::{
    dark_theme_available, system_theme, EffectiveTheme as RustEffectiveTheme,
    TextScale as RustTextScale, ThemeMode as RustThemeMode,
};

pub fn run() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let mut navigation = NavigationState::new();
    let mode = RustThemeMode::DEFAULT;
    let effective = mode.resolve(system_theme(), dark_theme_available());

    {
        let theme = ui.global::<Theme>();
        theme.set_mode(slint_theme_mode(mode));
        theme.set_effective_theme(slint_effective_theme(effective));
        theme.set_text_scale(slint_text_scale(RustTextScale::Default));
    }

    {
        let i18n = ui.global::<I18n>();
        set_i18n(&i18n, Locale::DEFAULT);
    }

    let theme_timer = if mode == RustThemeMode::System {
        // 轮询 Windows 主题设置，更新令牌而不重建窗口或丢失页面状态。
        let ui_weak = ui.as_weak();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(1),
            move || {
                if let Some(ui) = ui_weak.upgrade() {
                    let effective = mode.resolve(system_theme(), dark_theme_available());
                    ui.global::<Theme>()
                        .set_effective_theme(slint_effective_theme(effective));
                }
            },
        );
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

    let result = ui.run();
    drop(theme_timer);
    result
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
    // 批量设置共享 global，组件会自动刷新，不需要重建窗口。
    i18n.set_locale(locale.as_str().into());
    i18n.set_app_title(I18nCatalog::text(locale, TextKey::AppTitle).into());
    i18n.set_nav_welcome(I18nCatalog::text(locale, TextKey::NavWelcome).into());
    i18n.set_nav_configure(I18nCatalog::text(locale, TextKey::NavConfigure).into());
    i18n.set_nav_search(I18nCatalog::text(locale, TextKey::NavSearch).into());
    i18n.set_nav_tasks(I18nCatalog::text(locale, TextKey::NavTasks).into());
}
