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
    WelcomeTitle,
    WelcomeIntroduction,
    WelcomeStepConfigureNumber,
    WelcomeStepConfigureDescription,
    WelcomeStepConfigurePage,
    WelcomeStepSearchNumber,
    WelcomeStepSearchDescription,
    WelcomeStepSearchPage,
    WelcomeStepTasksDescription,
    WelcomeStepTasksPage,
    WelcomeStepTasksNumber,
    WelcomeDependenciesTitle,
    WelcomeDependencySlint,
    WelcomeDependencyWebbrowser,
    WelcomeDependencyWindowsSys,
    WelcomeDependencySlintBuild,
    WelcomeThanks,
    WelcomeProjectLabel,
    WelcomeProjectUrl,
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
            (Locale::ZhCn, TextKey::WelcomeTitle) => "欢迎使用 yt-dlp 图形界面",
            (Locale::ZhCn, TextKey::WelcomeIntroduction) => "只需以下三步，即可开始下载视频：",
            (Locale::ZhCn, TextKey::WelcomeStepConfigureNumber) => "1.",
            (Locale::ZhCn, TextKey::WelcomeStepConfigureDescription) => "页面设置所需的环境变量。",
            (Locale::ZhCn, TextKey::WelcomeStepConfigurePage) => "配置",
            (Locale::ZhCn, TextKey::WelcomeStepSearchNumber) => "2.",
            (Locale::ZhCn, TextKey::WelcomeStepSearchDescription) => {
                "页面输入 YouTube 视频链接，选择所需的流后开始下载。"
            }
            (Locale::ZhCn, TextKey::WelcomeStepSearchPage) => "检索",
            (Locale::ZhCn, TextKey::WelcomeStepTasksNumber) => "3.",
            (Locale::ZhCn, TextKey::WelcomeStepTasksDescription) => {
                "页面查看进行中和已完成的下载任务。"
            }
            (Locale::ZhCn, TextKey::WelcomeStepTasksPage) => "任务",
            (Locale::ZhCn, TextKey::WelcomeDependenciesTitle) => "依赖",
            (Locale::ZhCn, TextKey::WelcomeDependencySlint) => "slint",
            (Locale::ZhCn, TextKey::WelcomeDependencyWebbrowser) => "webbrowser",
            (Locale::ZhCn, TextKey::WelcomeDependencyWindowsSys) => "windows-sys",
            (Locale::ZhCn, TextKey::WelcomeDependencySlintBuild) => "slint-build",
            (Locale::ZhCn, TextKey::WelcomeThanks) => "感谢这些开源项目的贡献。",
            (Locale::ZhCn, TextKey::WelcomeProjectLabel) => "项目主页：",
            (Locale::ZhCn, TextKey::WelcomeProjectUrl) => "https://github.com/Kylineyes/yt-dlp-gui",
            (Locale::EnUs, TextKey::AppTitle) => "yt-dlp GUI",
            (Locale::EnUs, TextKey::NavWelcome) => "Welcome",
            (Locale::EnUs, TextKey::NavConfigure) => "Configure",
            (Locale::EnUs, TextKey::NavSearch) => "Search",
            (Locale::EnUs, TextKey::NavTasks) => "Tasks",
            (Locale::EnUs, TextKey::WelcomeTitle) => "Welcome to yt-dlp GUI",
            (Locale::EnUs, TextKey::WelcomeIntroduction) => {
                "Start downloading videos in three steps:"
            }
            (Locale::EnUs, TextKey::WelcomeStepConfigureNumber) => "1.",
            (Locale::EnUs, TextKey::WelcomeStepConfigureDescription) => {
                " page to set the required environment variables."
            }
            (Locale::EnUs, TextKey::WelcomeStepConfigurePage) => "Configure",
            (Locale::EnUs, TextKey::WelcomeStepSearchNumber) => "2.",
            (Locale::EnUs, TextKey::WelcomeStepSearchDescription) => {
                " page to enter a YouTube video URL, choose a stream, and start the download."
            }
            (Locale::EnUs, TextKey::WelcomeStepSearchPage) => "Search",
            (Locale::EnUs, TextKey::WelcomeStepTasksNumber) => "3.",
            (Locale::EnUs, TextKey::WelcomeStepTasksDescription) => {
                " page to view active and completed download tasks."
            }
            (Locale::EnUs, TextKey::WelcomeStepTasksPage) => "Tasks",
            (Locale::EnUs, TextKey::WelcomeDependenciesTitle) => "Dependencies",
            (Locale::EnUs, TextKey::WelcomeDependencySlint) => "slint",
            (Locale::EnUs, TextKey::WelcomeDependencyWebbrowser) => "webbrowser",
            (Locale::EnUs, TextKey::WelcomeDependencyWindowsSys) => "windows-sys",
            (Locale::EnUs, TextKey::WelcomeDependencySlintBuild) => "slint-build",
            (Locale::EnUs, TextKey::WelcomeThanks) => "Thank you to these open-source projects.",
            (Locale::EnUs, TextKey::WelcomeProjectLabel) => "Project home:",
            (Locale::EnUs, TextKey::WelcomeProjectUrl) => "https://github.com/Kylineyes/yt-dlp-gui",
        }
    }

    /// 保留显式的回退入口，便于后续接入可缺失的外部翻译资源。
    pub const fn text_or_default(locale: Locale, key: TextKey) -> &'static str {
        Self::text(locale, key)
    }
}
