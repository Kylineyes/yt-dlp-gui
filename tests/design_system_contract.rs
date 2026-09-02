// 这些测试锁定设计系统与页面分支之间的稳定数据契约，不启动真实窗口。
use yt_dlp_gui::app::contracts::Route;
use yt_dlp_gui::app::navigation::NavigationState;
use yt_dlp_gui::design_system::{EffectiveTheme, I18nCatalog, Locale, TextKey, TextScale, ThemeMode};

#[test]
fn theme_mode_resolution_has_safe_light_fallback() {
    assert_eq!(ThemeMode::DEFAULT, ThemeMode::System);
    assert_eq!(
        ThemeMode::Light.resolve(Some(EffectiveTheme::Dark), true),
        EffectiveTheme::Light
    );
    assert_eq!(
        ThemeMode::Dark.resolve(Some(EffectiveTheme::Light), true),
        EffectiveTheme::Dark
    );
    assert_eq!(
        ThemeMode::Dark.resolve(Some(EffectiveTheme::Dark), false),
        EffectiveTheme::Light
    );
    assert_eq!(
        ThemeMode::System.resolve(Some(EffectiveTheme::Dark), true),
        EffectiveTheme::Dark
    );
    assert_eq!(
        ThemeMode::System.resolve(Some(EffectiveTheme::Dark), false),
        EffectiveTheme::Light
    );
    assert_eq!(ThemeMode::System.resolve(None, true), EffectiveTheme::Light);
}

#[test]
fn theme_values_have_stable_serialization() {
    assert_eq!(ThemeMode::parse("light"), ThemeMode::Light);
    assert_eq!(ThemeMode::parse("DARK"), ThemeMode::Dark);
    assert_eq!(ThemeMode::parse("unknown"), ThemeMode::System);
    assert_eq!(ThemeMode::Dark.as_str(), "dark");
    assert_eq!(TextScale::parse("large").factor(), 1.15);
    assert_eq!(TextScale::parse("extra-large").factor(), 1.30);
    assert_eq!(TextScale::parse("unknown").factor(), 1.0);
}

#[test]
fn i18n_catalog_has_non_empty_bilingual_fallbacks() {
    assert_eq!(Locale::DEFAULT, Locale::EnUs);
    assert_eq!(Locale::parse("en-US"), Locale::EnUs);
    assert_eq!(Locale::parse("zh_CN"), Locale::ZhCn);
    assert_eq!(Locale::parse("unknown"), Locale::DEFAULT);

    for key in [
        TextKey::AppTitle,
        TextKey::NavWelcome,
        TextKey::NavConfigure,
        TextKey::NavSearch,
        TextKey::NavTasks,
        TextKey::WelcomeTitle,
        TextKey::WelcomeIntroduction,
        TextKey::WelcomeStepConfigureNumber,
        TextKey::WelcomeStepConfigureDescription,
        TextKey::WelcomeStepConfigurePage,
        TextKey::WelcomeStepSearchNumber,
        TextKey::WelcomeStepSearchDescription,
        TextKey::WelcomeStepSearchPage,
        TextKey::WelcomeStepTasksNumber,
        TextKey::WelcomeStepTasksDescription,
        TextKey::WelcomeStepTasksPage,
        TextKey::WelcomeDependenciesTitle,
        TextKey::WelcomeDependenciesRuntimeTitle,
        TextKey::WelcomeDependencyRusqlite,
        TextKey::WelcomeDependencySerdeJson,
        TextKey::WelcomeDependencySlint,
        TextKey::WelcomeDependencyWebbrowser,
        TextKey::WelcomeDependencyRfd,
        TextKey::WelcomeDependenciesBuildTitle,
        TextKey::WelcomeDependencySlintBuild,
        TextKey::WelcomeDependenciesWindowsTitle,
        TextKey::WelcomeDependencyWindowsSys,
        TextKey::WelcomeThanks,
        TextKey::WelcomeProjectLabel,
        TextKey::WelcomeProjectUrl,
        TextKey::ConfigureTitle,
        TextKey::ConfigureIntroduction,
        TextKey::ConfigureYtdlpPathLabel,
        TextKey::ConfigureYtdlpPathPlaceholder,
        TextKey::ConfigureBrowserLabel,
        TextKey::ConfigureLanguageLabel,
        TextKey::ConfigureThemeLabel,
        TextKey::ConfigureThemeSystem,
        TextKey::ConfigureThemeLight,
        TextKey::ConfigureThemeDark,
        TextKey::ConfigureSave,
        TextKey::ConfigureReset,
        TextKey::ConfigureLoading,
        TextKey::ConfigureSaving,
        TextKey::ConfigureSaved,
        TextKey::ConfigureValidationError,
        TextKey::ConfigureStorageError,
        TextKey::ConfigureProgramSettings,
        TextKey::ConfigureDownloadSettings,
        TextKey::ConfigureThirdParty,
        TextKey::ConfigureFfmpegPathLabel,
        TextKey::ConfigureFfmpegPathPlaceholder,
        TextKey::ConfigureDownloadPathLabel,
        TextKey::ConfigureDownloadPathPlaceholder,
        TextKey::ConfigureProxyLabel,
        TextKey::ConfigureProxyPlaceholder,
        TextKey::ConfigureConcurrentLabel,
        TextKey::ConfigureConcurrentPlaceholder,
        TextKey::ConfigureSearchTimeoutLabel,
        TextKey::ConfigureLanguageEnglish,
        TextKey::ConfigureLanguageChinese,
        TextKey::ConfigureBrowseFile,
        TextKey::ConfigureBrowseFolder,
        TextKey::ConfigureAutoFind,
        TextKey::ConfigureConcurrentHelp,
        TextKey::ConfigureErrorRequired,
        TextKey::ConfigureErrorWhitespace,
        TextKey::ConfigureErrorMissingFile,
        TextKey::ConfigureErrorNotFile,
        TextKey::ConfigureErrorMissingDirectory,
        TextKey::ConfigureErrorNotDirectory,
        TextKey::ConfigureErrorInvalidNumber,
        TextKey::ConfigureErrorInvalidOption,
        TextKey::ConfigureErrorInvalidToolName,
        TextKey::ConfigureErrorInvalidToolExtension,
        TextKey::ConfigureToolNotFound,
        TextKey::ConfigurePickerCancelled,
        TextKey::ConfigurePickerFailed,
        TextKey::ConfigureSearching,
        TextKey::SearchTitle,
        TextKey::SearchIntroduction,
        TextKey::SearchUrlLabel,
        TextKey::SearchUrlPlaceholder,
        TextKey::SearchStart,
        TextKey::SearchStop,
        TextKey::SearchDownloadPathLabel,
        TextKey::SearchDownloadPathPlaceholder,
        TextKey::SearchBrowseFolder,
        TextKey::SearchUseDefaultPath,
        TextKey::SearchStartDownload,
        TextKey::SearchResultsTitle,
        TextKey::SearchFormatId,
        TextKey::SearchFormatNote,
        TextKey::SearchExtension,
        TextKey::SearchResolution,
        TextKey::SearchBitrate,
        TextKey::SearchFileSize,
        TextKey::SearchVideoCodec,
        TextKey::SearchAudioCodec,
        TextKey::SearchVideoTitle,
        TextKey::SearchNoResults,
        TextKey::SearchSearchingTemplate,
        TextKey::SearchSuccess,
        TextKey::SearchFailed,
        TextKey::SearchCancelled,
        TextKey::SearchTimeout,
        TextKey::SearchErrorPathWhitespace,
        TextKey::SearchErrorPathMissing,
        TextKey::SearchErrorPathFile,
        TextKey::SearchErrorConfig,
        TextKey::SearchErrorYtdlp,
        TextKey::SearchErrorProcess,
        TextKey::SearchErrorMetadata,
        TextKey::SearchErrorUnexpected,
        TextKey::TasksTitle,
        TextKey::TasksIntroduction,
        TextKey::TasksTableTitle,
        TextKey::TasksColumnTitle,
        TextKey::TasksColumnStatus,
        TextKey::TasksColumnProgress,
        TextKey::TasksColumnSize,
        TextKey::TasksColumnSpeed,
        TextKey::TasksColumnEta,
        TextKey::TasksColumnUpdatedAt,
        TextKey::TasksColumnTargetPath,
        TextKey::TasksNoTasks,
        TextKey::TasksDeleteSelected,
        TextKey::TasksOpenVideoPath,
        TextKey::TasksDelete,
        TextKey::TasksRedownload,
        TextKey::TasksOpenVideoUrl,
        TextKey::TasksSelectAll,
        TextKey::TasksStatusPending,
        TextKey::TasksStatusPreparing,
        TextKey::TasksStatusDownloading,
        TextKey::TasksStatusMerging,
        TextKey::TasksStatusCompleted,
        TextKey::TasksStatusCancelled,
        TextKey::TasksStatusFailed,
        TextKey::TableResetWidths,
        TextKey::TableResetTitles,
        TextKey::TableShowColumns,
    ] {
        assert!(!I18nCatalog::text(Locale::ZhCn, key).is_empty());
        assert!(!I18nCatalog::text(Locale::EnUs, key).is_empty());
    }
}

#[test]
fn concurrent_limit_copy_describes_unlimited_zero() {
    assert_eq!(
        I18nCatalog::text(Locale::ZhCn, TextKey::ConfigureConcurrentHelp),
        "0 表示无并发限制。"
    );
    assert_eq!(
        I18nCatalog::text(Locale::EnUs, TextKey::ConfigureConcurrentHelp),
        "0 means no concurrency limit."
    );
}

#[test]
fn tasks_status_copy_matches_the_stable_bilingual_contract() {
    let statuses = [
        (TextKey::TasksStatusPending, "待开始", "Pending"),
        (TextKey::TasksStatusPreparing, "准备中", "Preparing"),
        (TextKey::TasksStatusDownloading, "下载中", "Downloading"),
        (TextKey::TasksStatusMerging, "合并中", "Merging"),
        (TextKey::TasksStatusCompleted, "已完成", "Completed"),
        (TextKey::TasksStatusCancelled, "已取消", "Cancelled"),
        (TextKey::TasksStatusFailed, "失败", "Failed"),
    ];

    for (key, zh_cn, en_us) in statuses {
        assert_eq!(I18nCatalog::text(Locale::ZhCn, key), zh_cn);
        assert_eq!(I18nCatalog::text(Locale::EnUs, key), en_us);
    }
}

#[test]
fn tasks_action_copy_matches_the_stable_bilingual_contract() {
    assert_eq!(
        I18nCatalog::text(Locale::ZhCn, TextKey::TasksDeleteSelected),
        "删除选中"
    );
    assert_eq!(
        I18nCatalog::text(Locale::EnUs, TextKey::TasksDeleteSelected),
        "Delete selected"
    );
    let actions = [
        (TextKey::TasksOpenVideoPath, "打开视频存放路径", "Open video location"),
        (TextKey::TasksDelete, "删除此条任务", "Delete this task"),
        (TextKey::TasksRedownload, "重新下载", "Redownload"),
        (TextKey::TasksOpenVideoUrl, "打开视频链接", "Open video URL"),
        (TextKey::TasksSelectAll, "全选所有视频", "Select all videos"),
    ];
    for (key, zh_cn, en_us) in actions {
        assert_eq!(I18nCatalog::text(Locale::ZhCn, key), zh_cn);
        assert_eq!(I18nCatalog::text(Locale::EnUs, key), en_us);
    }
    assert_eq!(I18nCatalog::text(Locale::ZhCn, TextKey::TasksColumnEta), "预计剩余时间");
    assert_eq!(
        I18nCatalog::text(Locale::EnUs, TextKey::TasksColumnEta),
        "Estimated time remaining"
    );
}
#[test]
fn table_menu_copy_matches_the_stable_bilingual_contract() {
    let copy = [
        (TextKey::TableResetWidths, "恢复默认列宽", "Reset column widths"),
        (TextKey::TableResetTitles, "显示全部列", "Show all columns"),
        (TextKey::TableShowColumns, "显示列", "Show columns"),
    ];

    for (key, zh_cn, en_us) in copy {
        assert_eq!(I18nCatalog::text(Locale::ZhCn, key), zh_cn);
        assert_eq!(I18nCatalog::text(Locale::EnUs, key), en_us);
    }
}

#[test]
fn routes_remain_a_stable_four_page_contract() {
    assert_eq!(
        Route::ALL,
        [Route::Welcome, Route::Configure, Route::Search, Route::Tasks]
    );

    for (index, route) in Route::ALL.into_iter().enumerate() {
        assert_eq!(route.index(), index as i32);
        assert_eq!(Route::from_index(index as i32), Some(route));
    }

    assert_eq!(Route::from_index(-1), None);
    assert_eq!(Route::from_index(4), None);
}

#[test]
fn navigation_state_rejects_invalid_route_indices() {
    let mut state = NavigationState::new();
    assert_eq!(state.current(), Route::Welcome);
    assert!(state.navigate_to_index(2));
    assert_eq!(state.current(), Route::Search);
    assert!(!state.navigate_to_index(99));
    assert_eq!(state.current(), Route::Search);
}
