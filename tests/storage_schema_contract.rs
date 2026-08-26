use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use yt_dlp_gui::storage::schema::initialize_schema;
use yt_dlp_gui::storage::{EnvironmentConfig, Storage, StorageError, CONFIG_VERSION};

fn temporary_database_path() -> PathBuf {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("yt-dlp-gui-schema-{timestamp}.sqlite"))
}

#[test]
fn schema_creates_empty_config_table_without_seed_data() {
    let path = temporary_database_path();
    let connection = Connection::open(&path).unwrap();

    initialize_schema(&connection).unwrap();

    let table_count: i64 = connection
        .query_row(
            "
select
    count(*)
from
    sqlite_master
where
    type = 'table'
    and name = 'config'
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let row_count: i64 = connection
        .query_row(
            "
select
    count(*)
from
    config
",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(table_count, 1);
    assert_eq!(row_count, 0);
    assert!(path.is_file());
    drop(connection);
    fs::remove_file(path).unwrap();
}

#[test]
fn schema_adds_default_search_timeout_to_existing_config_table() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "
create table config (
    singleton integer primary key check (singleton = 1),
    version text not null,
    yt_dlp_path text not null,
    ffmpeg_path text not null,
    default_download_path text not null,
    theme text not null,
    language text not null,
    concurrent_downloads integer not null,
    proxy text not null
);

insert into config (
    singleton,
    version,
    yt_dlp_path,
    ffmpeg_path,
    default_download_path,
    theme,
    language,
    concurrent_downloads,
    proxy
)
values (
    1,
    '0.0.1',
    '',
    '',
    '',
    'system',
    'en-US',
    0,
    ''
);
",
        )
        .unwrap();

    initialize_schema(&connection).unwrap();

    let search_timeout_sec: i64 = connection
        .query_row(
            "
select
    search_timeout_sec
from
    config
where
    singleton = 1
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(search_timeout_sec, 20);
}

#[test]
fn config_singleton_upsert_replaces_the_existing_configuration() {
    let connection = Connection::open_in_memory().unwrap();
    initialize_schema(&connection).unwrap();

    for theme in ["system", "dark"] {
        connection
            .execute(
                "
insert into config (
    singleton,
    version,
    yt_dlp_path,
    ffmpeg_path,
    default_download_path,
    theme,
    language,
    concurrent_downloads,
    proxy
)
values (
    1,
    '0.0.1',
    '',
    '',
    '',
    ?1,
    'en-US',
    0,
    ''
)
on conflict (singleton) do update set
    theme = excluded.theme
",
                params![theme],
            )
            .unwrap();
    }

    let row_count: i64 = connection
        .query_row(
            "
select
    count(*)
from
    config
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let theme: String = connection
        .query_row(
            "
select
    theme
from
    config
where
    singleton = 1
",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let search_timeout_sec: i64 = connection
        .query_row(
            "
select
    search_timeout_sec
from
    config
where
    singleton = 1
",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(row_count, 1);
    assert_eq!(theme, "dark");
    assert_eq!(search_timeout_sec, 20);
}

#[test]
fn storage_exposes_search_timeout_getter_setter_and_range_validation() {
    let path = PathBuf::from("target").join(format!("storage-search-timeout-{}.sqlite", std::process::id()));
    let _ = fs::remove_file(&path);
    Storage::initialize(path).unwrap();
    let storage = Storage::instance().unwrap();

    storage
        .save_configuration(EnvironmentConfig {
            version: CONFIG_VERSION.to_owned(),
            yt_dlp_path: String::new(),
            ffmpeg_path: String::new(),
            default_download_path: String::new(),
            theme: "system".to_owned(),
            language: "en-US".to_owned(),
            concurrent_downloads: 0,
            proxy: String::new(),
            search_timeout_sec: 20,
        })
        .unwrap();
    assert_eq!(storage.search_timeout_sec().unwrap(), Some(20));

    storage.set_search_timeout_sec(5).unwrap();
    assert_eq!(storage.search_timeout_sec().unwrap(), Some(5));
    storage.set_search_timeout_sec(120).unwrap();
    assert_eq!(storage.search_timeout_sec().unwrap(), Some(120));

    assert!(matches!(
        storage.set_search_timeout_sec(4),
        Err(StorageError::InvalidSearchTimeout(4))
    ));
    assert_eq!(storage.search_timeout_sec().unwrap(), Some(120));
    assert!(matches!(
        storage.set_search_timeout_sec(121),
        Err(StorageError::InvalidSearchTimeout(121))
    ));
    assert_eq!(storage.search_timeout_sec().unwrap(), Some(120));
}
