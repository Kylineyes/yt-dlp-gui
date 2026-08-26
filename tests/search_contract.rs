use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use yt_dlp_gui::app::search::{
    can_download, classify_failure, format_row, next_sort_state, sorted_result_indices, validate_download_path,
    SearchFailure, SearchPathError, SortColumn, SortDirection,
};
use yt_dlp_gui::download_task::{DownloadTaskError, MediaFormat, VideoInfo};

fn unique_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "yt-dlp-gui-search-{name}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ))
}

#[test]
fn download_path_validation_matches_configuration_rules() {
    assert!(validate_download_path("").is_ok());
    assert_eq!(
        validate_download_path(" path "),
        Err(SearchPathError::LeadingOrTrailingWhitespace)
    );
    assert_eq!(
        validate_download_path("definitely-missing"),
        Err(SearchPathError::MissingDirectory)
    );

    let file = unique_path("file");
    fs::write(&file, b"test").unwrap();
    assert_eq!(
        validate_download_path(&file.display().to_string()),
        Err(SearchPathError::NotADirectory)
    );
    fs::remove_file(file).unwrap();

    let directory = unique_path("directory");
    fs::create_dir(&directory).unwrap();
    assert!(validate_download_path(&directory.display().to_string()).is_ok());
    fs::remove_dir(directory).unwrap();
}

#[test]
fn download_button_requires_selection_and_valid_non_empty_path() {
    assert!(!can_download("", Some(0), None));
    assert!(!can_download("C:\\downloads", None, None));
    assert!(!can_download(
        "C:\\downloads",
        Some(0),
        Some(SearchPathError::MissingDirectory)
    ));
    assert!(can_download("C:\\downloads", Some(0), None));
}

#[test]
fn format_rows_render_optional_values_and_size() {
    let row = format_row(&MediaFormat {
        format_id: Some("18".to_string()),
        format_note: Some("medium".to_string()),
        extension: Some("mp4".to_string()),
        resolution: Some("360p".to_string()),
        bitrate_kbps: Some(512.0),
        filesize: Some(1024 * 1024),
        ..MediaFormat {
            format_id: None,
            format_note: None,
            extension: None,
            resolution: None,
            width: None,
            height: None,
            fps: None,
            filesize: None,
            filesize_approx: None,
            bitrate_kbps: None,
            video_codec: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            video_bitrate_kbps: None,
            protocol: None,
            url: None,
        }
    });
    assert_eq!(row.format_id, "18");
    assert_eq!(row.bitrate, "512 Kbps");
    assert_eq!(row.file_size, "1.0 MiB");
    assert!(row.video_codec.is_empty());
    assert!(row.audio_codec.is_empty());

    let with_codecs = MediaFormat {
        video_codec: Some("avc1.640028".to_owned()),
        audio_codec: Some("mp4a.40.2".to_owned()),
        ..MediaFormat {
            format_id: None,
            format_note: None,
            extension: None,
            resolution: None,
            width: None,
            height: None,
            fps: None,
            filesize: None,
            filesize_approx: None,
            bitrate_kbps: None,
            video_codec: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            video_bitrate_kbps: None,
            protocol: None,
            url: None,
        }
    };
    let codec_row = format_row(&with_codecs);
    assert_eq!(codec_row.video_codec, "avc1.640028");
    assert_eq!(codec_row.audio_codec, "mp4a.40.2");
}

#[test]
fn failures_map_to_safe_ui_categories() {
    assert_eq!(
        classify_failure(&DownloadTaskError::ExecutableNotFound("yt-dlp.exe".into())),
        SearchFailure::YtDlpPathMissing
    );
    assert_eq!(
        classify_failure(&DownloadTaskError::InvalidJson("bad".to_string())),
        SearchFailure::Metadata
    );
    assert_eq!(
        classify_failure(&DownloadTaskError::Timeout(std::time::Duration::from_secs(20))),
        SearchFailure::TimedOut
    );
}

fn format(id: Option<&str>, bitrate: Option<f64>, size: Option<u64>, height: Option<u64>) -> MediaFormat {
    MediaFormat {
        format_id: id.map(str::to_owned),
        extension: Some("mp4".to_owned()),
        resolution: height.map(|height| format!("{height}p")),
        width: height.map(|height| height * 2),
        height,
        bitrate_kbps: bitrate,
        filesize: size,
        format_note: Some("note".to_owned()),
        ..MediaFormat {
            format_id: None,
            format_note: None,
            extension: None,
            resolution: None,
            width: None,
            height: None,
            fps: None,
            filesize: None,
            filesize_approx: None,
            bitrate_kbps: None,
            video_codec: None,
            audio_codec: None,
            audio_bitrate_kbps: None,
            video_bitrate_kbps: None,
            protocol: None,
            url: None,
        }
    }
}

fn sortable_video(formats: Vec<MediaFormat>) -> VideoInfo {
    VideoInfo {
        id: "id".to_owned(),
        title: "title".to_owned(),
        webpage_url: None,
        original_url: None,
        uploader: None,
        channel: None,
        duration_seconds: None,
        thumbnail_url: None,
        description: None,
        upload_date: None,
        formats,
    }
}

#[test]
fn sort_state_cycles_and_switches_columns() {
    assert_eq!(
        next_sort_state(None, SortDirection::Reset, SortColumn::Bitrate),
        (Some(SortColumn::Bitrate), SortDirection::Ascending)
    );
    assert_eq!(
        next_sort_state(Some(SortColumn::Bitrate), SortDirection::Ascending, SortColumn::Bitrate),
        (Some(SortColumn::Bitrate), SortDirection::Descending)
    );
    assert_eq!(
        next_sort_state(
            Some(SortColumn::Bitrate),
            SortDirection::Descending,
            SortColumn::Bitrate
        ),
        (None, SortDirection::Reset)
    );
    assert_eq!(
        next_sort_state(
            Some(SortColumn::Bitrate),
            SortDirection::Descending,
            SortColumn::FileSize
        ),
        (Some(SortColumn::FileSize), SortDirection::Ascending)
    );
}

#[test]
fn sorting_uses_numeric_values_and_reset_restores_original_order() {
    let video = sortable_video(vec![
        format(Some("18"), Some(512.0), Some(1024), Some(360)),
        format(Some("9"), Some(50.0), Some(2 * 1024 * 1024), Some(1080)),
        format(Some("140"), Some(90.0), Some(1024 * 1024), Some(720)),
    ]);
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::Bitrate), SortDirection::Ascending),
        vec![1, 2, 0]
    );
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::FormatId), SortDirection::Ascending),
        vec![1, 0, 2]
    );
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::FileSize), SortDirection::Descending),
        vec![1, 2, 0]
    );
    assert_eq!(sorted_result_indices(&video, None, SortDirection::Reset), vec![0, 1, 2]);
}

#[test]
fn sorting_keeps_missing_values_last_and_stable() {
    let video = sortable_video(vec![
        format(Some("a"), None, None, None),
        format(Some("b"), Some(100.0), None, None),
        format(Some("c"), None, None, None),
        format(Some("d"), Some(50.0), None, None),
    ]);
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::Bitrate), SortDirection::Ascending),
        vec![3, 1, 0, 2]
    );
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::Bitrate), SortDirection::Descending),
        vec![1, 3, 0, 2]
    );
}

#[test]
fn codec_sorting_is_case_insensitive_and_keeps_missing_values_last() {
    let video = sortable_video(vec![
        MediaFormat {
            video_codec: Some("vp9".to_owned()),
            audio_codec: Some("opus".to_owned()),
            ..format(Some("1"), None, None, None)
        },
        MediaFormat {
            video_codec: Some("AV1".to_owned()),
            audio_codec: Some("AAC".to_owned()),
            ..format(Some("2"), None, None, None)
        },
        MediaFormat {
            audio_codec: Some("mp3".to_owned()),
            ..format(Some("3"), None, None, None)
        },
    ]);

    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::VideoCodec), SortDirection::Ascending),
        vec![1, 0, 2]
    );
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::VideoCodec), SortDirection::Descending),
        vec![0, 1, 2]
    );
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::AudioCodec), SortDirection::Ascending),
        vec![1, 2, 0]
    );
    assert_eq!(
        sorted_result_indices(&video, Some(SortColumn::AudioCodec), SortDirection::Descending),
        vec![0, 2, 1]
    );
}

#[test]
fn video_info_contract_has_flat_format_rows() {
    let video = VideoInfo {
        id: "id".to_string(),
        title: "title".to_string(),
        webpage_url: None,
        original_url: None,
        uploader: None,
        channel: None,
        duration_seconds: None,
        thumbnail_url: None,
        description: None,
        upload_date: None,
        formats: Vec::new(),
    };
    assert!(yt_dlp_gui::app::search::result_rows(&video).is_empty());
}
