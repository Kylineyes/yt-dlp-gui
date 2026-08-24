use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use yt_dlp_gui::storage::schema::initialize_schema;

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

    assert_eq!(row_count, 1);
    assert_eq!(theme, "dark");
}
