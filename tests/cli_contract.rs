use std::ffi::OsString;
use std::path::PathBuf;

use yt_dlp_gui::cli::{AppArgs, CliError};

#[test]
fn parses_explicit_config_path() {
    let args = AppArgs::parse_from([OsString::from("-c"), OsString::from("custom.sqlite")]).unwrap();

    assert_eq!(args.config_path, Some(PathBuf::from("custom.sqlite")));
}

#[test]
fn preserves_missing_config_path_for_storage() {
    let args = AppArgs::parse_from([]).unwrap();

    assert_eq!(args.config_path, None);
}

#[test]
fn rejects_missing_config_path_value() {
    assert_eq!(
        AppArgs::parse_from([OsString::from("-c")]),
        Err(CliError::MissingConfigPath)
    );
}

#[test]
fn rejects_duplicate_config_path() {
    assert_eq!(
        AppArgs::parse_from([
            OsString::from("-c"),
            OsString::from("first.sqlite"),
            OsString::from("-c"),
            OsString::from("second.sqlite"),
        ]),
        Err(CliError::DuplicateConfigPath)
    );
}

#[test]
fn rejects_unexpected_argument() {
    assert_eq!(
        AppArgs::parse_from([OsString::from("--verbose")]),
        Err(CliError::UnexpectedArgument(OsString::from("--verbose")))
    );
}
