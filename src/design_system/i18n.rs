/// 应用支持的界面语言；字符串形式使用 BCP-47 locale 标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    ZhCn,
    EnUs,
}

impl Locale {
    /// 未配置语言时默认使用简体中文。
    pub const DEFAULT: Self = Self::ZhCn;

    /// 返回用于配置和 Slint global 的稳定 locale 标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }

    /// 解析常见 locale 别名；未知语言统一回退默认语言。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en_us" => Self::EnUs,
            "zh" | "zh-cn" | "zh_cn" | "zh-hans" => Self::ZhCn,
            _ => Self::DEFAULT,
        }
    }
}

/// 共享 UI 的可见文案键；页面不得直接硬编码对应文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKey {
    AppTitle,
    NavWelcome,
    NavConfigure,
    NavSearch,
    NavTasks,
}

/// 无状态的共享 catalog；所有 key 在首批语言中都有定义的回退文案。
#[derive(Debug, Clone, Copy, Default)]
pub struct I18nCatalog;

impl I18nCatalog {
    /// 按语言和 key 返回最终可显示文本，而不是返回未解析的 key。
    pub const fn text(locale: Locale, key: TextKey) -> &'static str {
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
        }
    }

    /// 保留显式的回退入口，便于后续接入可缺失的外部翻译资源。
    pub const fn text_or_default(locale: Locale, key: TextKey) -> &'static str {
        Self::text(locale, key)
    }
}
