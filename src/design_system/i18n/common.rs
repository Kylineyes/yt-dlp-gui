use super::locale::Locale;
use super::types::TextKey;

pub(super) const fn text(locale: Locale, key: TextKey) -> &'static str {
    match (locale, key) {
        (Locale::ZhCn, TextKey::AppTitle) => "yt-dlp 图形界面",
        (Locale::ZhCn, TextKey::NavWelcome) => "欢迎",
        (Locale::ZhCn, TextKey::NavConfigure) => "配置",
        (Locale::ZhCn, TextKey::NavSearch) => "检索",
        (Locale::ZhCn, TextKey::NavTasks) => "任务",
        (Locale::EnUs, TextKey::AppTitle) => "yt-dlp GUI",
        (Locale::EnUs, TextKey::NavWelcome) => "Welcome",
        (Locale::EnUs, TextKey::NavConfigure) => "Configure",
        (Locale::EnUs, TextKey::NavSearch) => "Search",
        (Locale::EnUs, TextKey::NavTasks) => "Tasks",
        _ => "",
    }
}
