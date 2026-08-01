use crate::app::state::AppSettings;

pub(super) const YT_DLP_PATH: &str = "yt_dlp_path";
pub(super) const FFMPEG_PATH: &str = "ffmpeg_path";
pub(super) const DEFAULT_DOWNLOAD_DIRECTORY: &str = "default_download_directory";
pub(super) const PROXY: &str = "proxy";
pub(super) const MAX_CONCURRENCY: &str = "max_concurrency";
pub(super) const LANGUAGE: &str = "language";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppConfigKey {
    YtDlpPath,
    FfmpegPath,
    DefaultDownloadDirectory,
    Proxy,
    MaxConcurrency,
    Language,
}

pub(super) const ALL_CONFIG_KEYS: [AppConfigKey; 6] = [
    AppConfigKey::YtDlpPath,
    AppConfigKey::FfmpegPath,
    AppConfigKey::DefaultDownloadDirectory,
    AppConfigKey::Proxy,
    AppConfigKey::MaxConcurrency,
    AppConfigKey::Language,
];

impl AppConfigKey {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::YtDlpPath => YT_DLP_PATH,
            Self::FfmpegPath => FFMPEG_PATH,
            Self::DefaultDownloadDirectory => DEFAULT_DOWNLOAD_DIRECTORY,
            Self::Proxy => PROXY,
            Self::MaxConcurrency => MAX_CONCURRENCY,
            Self::Language => LANGUAGE,
        }
    }

    pub(super) fn value_from(self, settings: &AppSettings) -> String {
        match self {
            Self::YtDlpPath => settings.yt_dlp_path.clone(),
            Self::FfmpegPath => settings.ffmpeg_path.clone(),
            Self::DefaultDownloadDirectory => settings.default_download_directory.clone(),
            Self::Proxy => settings.proxy.clone(),
            Self::MaxConcurrency => settings.max_concurrency.to_string(),
            Self::Language => settings.language.clone(),
        }
    }

    pub(super) fn apply_to(self, settings: &mut AppSettings, value: String) {
        match self {
            Self::YtDlpPath => settings.yt_dlp_path = value,
            Self::FfmpegPath => settings.ffmpeg_path = value,
            Self::DefaultDownloadDirectory => settings.default_download_directory = value,
            Self::Proxy => settings.proxy = value,
            Self::MaxConcurrency => {
                if let Ok(max_concurrency) = value.parse::<u32>() {
                    settings.max_concurrency = max_concurrency.clamp(1, 16);
                }
            }
            Self::Language => settings.language = value,
        }
    }
}
