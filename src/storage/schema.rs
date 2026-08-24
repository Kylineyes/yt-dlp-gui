use rusqlite::{Connection, Result, Transaction};

/// 下载任务相关表结构的独立版本，不与配置版本混用。
pub const DOWNLOAD_SCHEMA_VERSION: i64 = 1;

const CREATE_CONFIG_TABLE: &str = r#"
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
"#;

const CREATE_STORAGE_TABLES: &str = r#"
create table if not exists storage_schema_versions (
    domain text primary key,
    version integer not null check (version >= 1)
);

create table if not exists download_tasks (
    id integer primary key autoincrement,
    source_url text not null,
    video_id text,
    title text,
    thumbnail_url text,
    duration_seconds integer check (
        duration_seconds is null
        or duration_seconds >= 0
    ),
    target_path text not null,
    output_path text,
    selected_format text,
    status text not null check (
        status in (
            'pending',
            'preparing',
            'downloading',
            'merging',
            'completed',
            'cancelled',
            'failed'
        )
    ),
    progress_percent integer check (
        progress_percent is null
        or (
            progress_percent >= 0
            and progress_percent <= 100
        )
    ),
    downloaded_bytes integer not null default 0 check (downloaded_bytes >= 0),
    total_bytes integer check (total_bytes is null or total_bytes >= 0),
    total_bytes_estimate integer check (
        total_bytes_estimate is null
        or total_bytes_estimate >= 0
    ),
    speed_bytes_per_second integer check (
        speed_bytes_per_second is null
        or speed_bytes_per_second >= 0
    ),
    elapsed_seconds integer check (elapsed_seconds is null or elapsed_seconds >= 0),
    eta_seconds integer check (eta_seconds is null or eta_seconds >= 0),
    created_at integer not null,
    started_at integer,
    finished_at integer,
    updated_at integer not null,
    yt_dlp_version text,
    error_code text,
    error_message text
);

create table if not exists download_task_streams (
    id integer primary key autoincrement,
    task_id integer not null,
    stream_key text not null,
    format_id text,
    media_type text not null check (media_type in ('video', 'audio')),
    extension text,
    width integer check (width is null or width >= 0),
    height integer check (height is null or height >= 0),
    video_codec text,
    audio_codec text,
    status text not null check (
        status in (
            'pending',
            'preparing',
            'downloading',
            'merging',
            'completed',
            'cancelled',
            'failed'
        )
    ),
    progress_percent integer check (
        progress_percent is null
        or (
            progress_percent >= 0
            and progress_percent <= 100
        )
    ),
    downloaded_bytes integer not null default 0 check (downloaded_bytes >= 0),
    total_bytes integer check (total_bytes is null or total_bytes >= 0),
    total_bytes_estimate integer check (
        total_bytes_estimate is null
        or total_bytes_estimate >= 0
    ),
    speed_bytes_per_second integer check (
        speed_bytes_per_second is null
        or speed_bytes_per_second >= 0
    ),
    elapsed_seconds integer check (elapsed_seconds is null or elapsed_seconds >= 0),
    eta_seconds integer check (eta_seconds is null or eta_seconds >= 0),
    created_at integer not null,
    started_at integer,
    finished_at integer,
    updated_at integer not null,
    foreign key (task_id) references download_tasks(id) on delete cascade,
    unique (task_id, stream_key)
);

create index if not exists idx_download_tasks_status_updated
    on download_tasks(status, updated_at desc, id desc);

create index if not exists idx_download_tasks_created
    on download_tasks(created_at desc, id desc);

create index if not exists idx_download_task_streams_task
    on download_task_streams(task_id);

insert into storage_schema_versions (
    domain,
    version
)
values (
    'download_tasks',
    1
)
on conflict (domain) do nothing;
"#;

/// 创建或升级全部存储表；schema 变更与版本记录在同一事务中提交。
pub fn initialize_schema(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    initialize_config_schema(&transaction)?;
    transaction.execute_batch(CREATE_STORAGE_TABLES)?;
    let version: i64 = transaction.query_row(
        "
select
    version
from
    storage_schema_versions
where
    domain = 'download_tasks'
",
        [],
        |row| row.get(0),
    )?;
    if version > DOWNLOAD_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(
            "storage schema version".to_owned(),
        ));
    }
    transaction.commit()
}

/// 将旧的无单例约束配置表迁移为单行 UPSERT 所需的稳定主键结构。
fn initialize_config_schema(transaction: &Transaction<'_>) -> Result<()> {
    let config_exists: bool = transaction.query_row(
        "
select
    exists (
        select
            1
        from
            sqlite_master
        where
            type = 'table'
            and name = 'config'
    )
",
        [],
        |row| row.get(0),
    )?;
    if !config_exists {
        return transaction.execute_batch(CREATE_CONFIG_TABLE);
    }

    let has_singleton: bool = transaction.query_row(
        "
select
    exists (
        select
            1
        from
            pragma_table_info('config')
        where
            name = 'singleton'
    )
",
        [],
        |row| row.get(0),
    )?;
    if has_singleton {
        return Ok(());
    }

    transaction.execute_batch(
        "
alter table config rename to config_legacy;
",
    )?;
    transaction.execute_batch(CREATE_CONFIG_TABLE)?;
    transaction.execute_batch(
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
select
    1,
    version,
    yt_dlp_path,
    ffmpeg_path,
    default_download_path,
    theme,
    language,
    concurrent_downloads,
    proxy
from
    config_legacy
limit
    1;

drop table config_legacy;
",
    )
}
