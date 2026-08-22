use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
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
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'config'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let row_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM config", [], |row| row.get(0))
        .unwrap();

    assert_eq!(table_count, 1);
    assert_eq!(row_count, 0);
    assert!(path.is_file());
    drop(connection);
    fs::remove_file(path).unwrap();
}
