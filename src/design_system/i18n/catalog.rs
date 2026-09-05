use super::locale::Locale;
use super::types::TextKey;
use super::{common, configure, search, table, tasks, welcome};

#[derive(Debug, Clone, Copy, Default)]
pub struct I18nCatalog;

impl I18nCatalog {
    pub const fn text(locale: Locale, key: TextKey) -> &'static str {
        match key {
            TextKey::AppTitle
            | TextKey::NavWelcome
            | TextKey::NavConfigure
            | TextKey::NavSearch
            | TextKey::NavTasks => common::text(locale, key),
            TextKey::WelcomeTitle
            | TextKey::WelcomeIntroduction
            | TextKey::WelcomeStepConfigureNumber
            | TextKey::WelcomeStepConfigureDescription
            | TextKey::WelcomeStepConfigurePage
            | TextKey::WelcomeStepSearchNumber
            | TextKey::WelcomeStepSearchDescription
            | TextKey::WelcomeStepSearchPage
            | TextKey::WelcomeStepTasksNumber
            | TextKey::WelcomeStepTasksDescription
            | TextKey::WelcomeStepTasksPage
            | TextKey::WelcomeDependenciesTitle
            | TextKey::WelcomeDependenciesRuntimeTitle
            | TextKey::WelcomeDependencyRusqlite
            | TextKey::WelcomeDependencySerdeJson
            | TextKey::WelcomeDependencySlint
            | TextKey::WelcomeDependencyWebbrowser
            | TextKey::WelcomeDependencyRfd
            | TextKey::WelcomeDependenciesBuildTitle
            | TextKey::WelcomeDependencySlintBuild
            | TextKey::WelcomeDependenciesWindowsTitle
            | TextKey::WelcomeDependencyWindowsSys
            | TextKey::WelcomeThanks
            | TextKey::WelcomeProjectLabel
            | TextKey::WelcomeProjectUrl => welcome::text(locale, key),
            TextKey::ConfigureTitle
            | TextKey::ConfigureIntroduction
            | TextKey::ConfigureYtdlpPathLabel
            | TextKey::ConfigureYtdlpPathPlaceholder
            | TextKey::ConfigureBrowserLabel
            | TextKey::ConfigureLanguageLabel
            | TextKey::ConfigureThemeLabel
            | TextKey::ConfigureThemeSystem
            | TextKey::ConfigureThemeLight
            | TextKey::ConfigureThemeDark
            | TextKey::ConfigureSave
            | TextKey::ConfigureReset
            | TextKey::ConfigureLoading
            | TextKey::ConfigureSaving
            | TextKey::ConfigureSaved
            | TextKey::ConfigureValidationError
            | TextKey::ConfigureStorageError
            | TextKey::ConfigureProgramSettings
            | TextKey::ConfigureDownloadSettings
            | TextKey::ConfigureThirdParty
            | TextKey::ConfigureFfmpegPathLabel
            | TextKey::ConfigureFfmpegPathPlaceholder
            | TextKey::ConfigureDownloadPathLabel
            | TextKey::ConfigureDownloadPathPlaceholder
            | TextKey::ConfigureProxyLabel
            | TextKey::ConfigureProxyPlaceholder
            | TextKey::ConfigureConcurrentLabel
            | TextKey::ConfigureConcurrentPlaceholder
            | TextKey::ConfigureSearchTimeoutLabel
            | TextKey::ConfigureLanguageEnglish
            | TextKey::ConfigureLanguageChinese
            | TextKey::ConfigureBrowseFile
            | TextKey::ConfigureBrowseFolder
            | TextKey::ConfigureAutoFind
            | TextKey::ConfigureConcurrentHelp
            | TextKey::ConfigureErrorRequired
            | TextKey::ConfigureErrorWhitespace
            | TextKey::ConfigureErrorMissingFile
            | TextKey::ConfigureErrorNotFile
            | TextKey::ConfigureErrorMissingDirectory
            | TextKey::ConfigureErrorNotDirectory
            | TextKey::ConfigureErrorInvalidNumber
            | TextKey::ConfigureErrorInvalidOption
            | TextKey::ConfigureErrorInvalidToolName
            | TextKey::ConfigureErrorInvalidToolExtension
            | TextKey::ConfigureToolNotFound
            | TextKey::ConfigurePickerCancelled
            | TextKey::ConfigurePickerFailed
            | TextKey::ConfigureSearching => configure::text(locale, key),
            TextKey::SearchTitle
            | TextKey::SearchIntroduction
            | TextKey::SearchUrlLabel
            | TextKey::SearchUrlPlaceholder
            | TextKey::SearchStart
            | TextKey::SearchStop
            | TextKey::SearchDownloadPathLabel
            | TextKey::SearchDownloadPathPlaceholder
            | TextKey::SearchBrowseFolder
            | TextKey::SearchUseDefaultPath
            | TextKey::SearchStartDownload
            | TextKey::SearchResultsTitle
            | TextKey::SearchFormatId
            | TextKey::SearchFormatNote
            | TextKey::SearchExtension
            | TextKey::SearchResolution
            | TextKey::SearchBitrate
            | TextKey::SearchFileSize
            | TextKey::SearchVideoCodec
            | TextKey::SearchAudioCodec
            | TextKey::SearchVideoTitle
            | TextKey::SearchNoResults
            | TextKey::SearchSearchingTemplate
            | TextKey::SearchSuccess
            | TextKey::SearchFailed
            | TextKey::SearchCancelled
            | TextKey::SearchTimeout
            | TextKey::SearchErrorPathWhitespace
            | TextKey::SearchErrorPathMissing
            | TextKey::SearchErrorPathFile
            | TextKey::SearchErrorConfig
            | TextKey::SearchErrorYtdlp
            | TextKey::SearchErrorProcess
            | TextKey::SearchErrorMetadata
            | TextKey::SearchErrorUnexpected => search::text(locale, key),
            TextKey::TasksTitle
            | TextKey::TasksIntroduction
            | TextKey::TasksTableTitle
            | TextKey::TasksColumnTitle
            | TextKey::TasksColumnStatus
            | TextKey::TasksColumnProgress
            | TextKey::TasksColumnSize
            | TextKey::TasksColumnSpeed
            | TextKey::TasksColumnEta
            | TextKey::TasksColumnUpdatedAt
            | TextKey::TasksColumnTargetPath
            | TextKey::TasksNoTasks
            | TextKey::TasksDeleteSelected
            | TextKey::TasksOpenVideoPath
            | TextKey::TasksDelete
            | TextKey::TasksRedownload
            | TextKey::TasksOpenVideoUrl
            | TextKey::TasksCopyYtDlpCommand
            | TextKey::TasksSelectAll
            | TextKey::TasksStatusPending
            | TextKey::TasksStatusPreparing
            | TextKey::TasksStatusDownloading
            | TextKey::TasksStatusPaused
            | TextKey::TasksStatusMerging
            | TextKey::TasksStatusCompleted
            | TextKey::TasksStatusCancelled
            | TextKey::TasksStatusFailed => tasks::text(locale, key),
            TextKey::TableResetWidths | TextKey::TableResetTitles | TextKey::TableShowColumns => {
                table::text(locale, key)
            }
        }
    }

    pub const fn text_or_default(locale: Locale, key: TextKey) -> &'static str {
        Self::text(locale, key)
    }
}
