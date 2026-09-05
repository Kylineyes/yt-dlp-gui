use rusqlite::{Connection, OptionalExtension, Result, Transaction};

/// 下载任务相关表结构的独立版本，不与配置版本混用。
pub const DOWNLOAD_SCHEMA_VERSION: i64 = 2;

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
    proxy text not null,
    search_timeout_sec integer not null default 20 check (
        search_timeout_sec >= 5
        and search_timeout_sec <= 120
    )
);
"#;

const CREATE_STORAGE_VERSION_TABLE: &str = r#"
create table if not exists storage_schema_versions (
    domain text primary key,
    version integer not null check (version >= 1)
);
"#;

const CREATE_DOWNLOAD_TABLES: &str = r#"
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
            'paused',
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
            'paused',
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

create table if not exists download_task_execution_snapshots (
    task_id integer primary key,
    source_url text not null,
    video_format_id text not null,
    audio_format_id text not null,
    output_template text not null,
    target_directory text not null,
    temporary_directory text not null,
    merge_output_format text not null check (merge_output_format in ('mp4', 'mkv')),
    rate_limit text,
    retries integer check (retries is null or retries >= 0),
    fragment_retries integer check (fragment_retries is null or fragment_retries >= 0),
    file_access_retries integer check (file_access_retries is null or file_access_retries >= 0),
    concurrent_fragments integer check (concurrent_fragments is null or concurrent_fragments >= 0),
    foreign key (task_id) references download_tasks(id) on delete cascade
);

create index if not exists idx_download_tasks_status_updated
    on download_tasks(status, updated_at desc, id desc);

create index if not exists idx_download_tasks_created
    on download_tasks(created_at desc, id desc);

create index if not exists idx_download_task_streams_task
    on download_task_streams(task_id);

create trigger if not exists prevent_download_execution_snapshot_update
before update on download_task_execution_snapshots
begin
    select raise(abort, 'download execution snapshot is immutable');
end;
"#;

const INSERT_DOWNLOAD_SCHEMA_VERSION: &str = r#"
insert into storage_schema_versions (
    domain,
    version
)
values (
    'download_tasks',
    2
)
on conflict (domain) do nothing;
"#;

/// 创建或升级全部存储表；schema 变更与版本记录在同一事务中提交。
pub fn initialize_schema(connection: &Connection) -> Result<()> {
    // 外键开关不能在事务中修改；拒绝嵌套调用，避免迁移时发生隐式级联删除。
    if !connection.is_autocommit() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.execute_batch(
        "
pragma foreign_keys = on;
",
    )?;
    let requires_download_migration = download_schema_version(connection)? == Some(1);
    if requires_download_migration {
        connection.execute_batch(
            "
pragma foreign_keys = off;
",
        )?;
        let migration_result = migrate_download_schema(connection);
        let foreign_key_result = connection.execute_batch(
            "
pragma foreign_keys = on;
",
        );
        migration_result?;
        foreign_key_result?;
        return verify_foreign_keys(connection);
    }

    let transaction = connection.unchecked_transaction()?;
    initialize_config_schema(&transaction)?;
    transaction.execute_batch(CREATE_STORAGE_VERSION_TABLE)?;
    transaction.execute_batch(CREATE_DOWNLOAD_TABLES)?;
    transaction.execute_batch(INSERT_DOWNLOAD_SCHEMA_VERSION)?;
    let version = download_schema_version_in_transaction(&transaction)?;
    if version > DOWNLOAD_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(
            "storage schema version".to_owned(),
        ));
    }
    verify_foreign_keys(&transaction)?;
    transaction.commit()
}

fn migrate_download_schema(connection: &Connection) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;
    initialize_config_schema(&transaction)?;
    transaction.execute_batch(
        "
alter table download_task_streams rename to download_task_streams_legacy;
alter table download_tasks rename to download_tasks_legacy;
",
    )?;
    transaction.execute_batch(CREATE_DOWNLOAD_TABLES)?;
    transaction.execute_batch(
        "
insert into download_tasks (
    id,
    source_url,
    video_id,
    title,
    thumbnail_url,
    duration_seconds,
    target_path,
    output_path,
    selected_format,
    status,
    progress_percent,
    downloaded_bytes,
    total_bytes,
    total_bytes_estimate,
    speed_bytes_per_second,
    elapsed_seconds,
    eta_seconds,
    created_at,
    started_at,
    finished_at,
    updated_at,
    yt_dlp_version,
    error_code,
    error_message
)
select
    id,
    source_url,
    video_id,
    title,
    thumbnail_url,
    duration_seconds,
    target_path,
    output_path,
    selected_format,
    status,
    progress_percent,
    downloaded_bytes,
    total_bytes,
    total_bytes_estimate,
    speed_bytes_per_second,
    elapsed_seconds,
    eta_seconds,
    created_at,
    started_at,
    finished_at,
    updated_at,
    yt_dlp_version,
    error_code,
    error_message
from
    download_tasks_legacy;

insert into download_task_streams (
    id,
    task_id,
    stream_key,
    format_id,
    media_type,
    extension,
    width,
    height,
    video_codec,
    audio_codec,
    status,
    progress_percent,
    downloaded_bytes,
    total_bytes,
    total_bytes_estimate,
    speed_bytes_per_second,
    elapsed_seconds,
    eta_seconds,
    created_at,
    started_at,
    finished_at,
    updated_at
)
select
    id,
    task_id,
    stream_key,
    format_id,
    media_type,
    extension,
    width,
    height,
    video_codec,
    audio_codec,
    case
        when status = 'merging' then 'paused'
        else status
    end,
    progress_percent,
    downloaded_bytes,
    total_bytes,
    total_bytes_estimate,
    case when status = 'merging' then null else speed_bytes_per_second end,
    case when status = 'merging' then null else elapsed_seconds end,
    case when status = 'merging' then null else eta_seconds end,
    created_at,
    started_at,
    finished_at,
    updated_at
from
    download_task_streams_legacy;

update
    storage_schema_versions
set
    version = 2
where
    domain = 'download_tasks';
",
    )?;
    // 保留已经删除的最高 ID，迁移后不得把旧任务 ID 分配给新任务。
    for (name, legacy) in [
        ("download_tasks", "download_tasks_legacy"),
        ("download_task_streams", "download_task_streams_legacy"),
    ] {
        transaction.execute(
            "
insert into sqlite_sequence (
    name,
    seq
)
select
    ?1,
    0
where
    not exists (
        select
            1
        from
            sqlite_sequence
        where
            name = ?1
    )
",
            [name],
        )?;
        transaction.execute(
            "
update
    sqlite_sequence
set
    seq = max(seq, coalesce((
        select
            seq
        from
            sqlite_sequence
        where
            name = ?1
    ), 0))
where
    name = ?2
",
            [legacy, name],
        )?;
    }
    transaction.execute_batch(
        "
drop table download_task_streams_legacy;
drop table download_tasks_legacy;
",
    )?;
    // 旧索引随旧表删除后才能用相同名称为新表创建索引。
    transaction.execute_batch(CREATE_DOWNLOAD_TABLES)?;
    verify_foreign_key_data(&transaction)?;
    transaction.commit()
}

fn download_schema_version(connection: &Connection) -> Result<Option<i64>> {
    let storage_versions_exist: bool = connection.query_row(
        "
select
    exists (
        select
            1
        from
            sqlite_master
        where
            type = 'table'
            and name = 'storage_schema_versions'
    )
",
        [],
        |row| row.get(0),
    )?;
    if !storage_versions_exist {
        return Ok(None);
    }
    connection
        .query_row(
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
        )
        .optional()
}

fn download_schema_version_in_transaction(transaction: &Transaction<'_>) -> Result<i64> {
    transaction.query_row(
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
    )
}

fn verify_foreign_keys(connection: &Connection) -> Result<()> {
    let foreign_keys_enabled: i64 = connection.query_row(
        "
pragma foreign_keys
",
        [],
        |row| row.get(0),
    )?;
    if foreign_keys_enabled != 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    verify_foreign_key_data(connection)
}

fn verify_foreign_key_data(connection: &Connection) -> Result<()> {
    let violations: bool = connection.query_row(
        "
select
    exists (
        select
            1
        from
            pragma_foreign_key_check
    )
",
        [],
        |row| row.get(0),
    )?;
    if violations {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
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
        let has_search_timeout: bool = transaction.query_row(
            "
select
    exists (
        select
            1
        from
            pragma_table_info('config')
        where
            name = 'search_timeout_sec'
    )
",
            [],
            |row| row.get(0),
        )?;
        if !has_search_timeout {
            transaction.execute_batch(
                "
alter table config
add column search_timeout_sec integer not null default 20 check (
    search_timeout_sec >= 5
    and search_timeout_sec <= 120
);
",
            )?;
        }
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
