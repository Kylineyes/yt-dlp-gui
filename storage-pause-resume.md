# 单任务暂停与继续：存储契约与跨分支修改意向

## 1. 当前结论

本轮已补齐 `feature/storage` 的暂停/继续持久化实现和独立存储验证。状态、执行快照、原子 API、启动恢复、v1→v2 迁移及事务回滚均已落地。

这不等于整仓或端到端暂停/继续功能已经完成：当前整仓仍存在三个跨模块的 `Paused` 穷尽匹配编译错误，真实 yt-dlp 暂停与续传由 `feature/download_task` 实现，页面装配由所属分支接入。

本文件描述当前工作区接口；依赖分支实际接入前，应确认这些变更已通过正式提交同步到其基线。不要只凭主线已经包含暂停文案就认为存储契约已发布。

## 2. feature/storage 已完成事项

### 状态与进度

- `DownloadTaskStatus::Paused` 为非终态、非运行态。
- 普通暂停仅接受 `Preparing`、`Downloading`，不接受 `Pending`、`Merging` 或终态。
- 原子暂停任务及其 `Pending`、`Preparing`、`Downloading` 流；已完成、取消、失败的流不回退。
- 专用恢复仅接受有执行快照的 `Paused` 任务，将任务及暂停流转为 `Preparing`。
- 普通任务/流状态更新不能代替专用暂停与恢复接口。
- 暂停任务、暂停流拒绝进度写入；流写入也检查父任务状态。
- 暂停、终态及合并中的任务不能通过新增下载流绕过状态限制。
- 暂停和恢复保留 task_id、stream_id、累计字节、总大小、百分比及首次 started_at，不写入 finished_at。
- 暂停时清空速度、ETA、会话耗时；已完成流完整保留。
- `Completed`、`Cancelled`、`Failed` 不可恢复；暂停任务和暂停流可转为取消或失败。

### 执行快照

`DownloadExecutionSnapshot` 包含：

- `source_url`；
- `video_format_id`、`audio_format_id`；
- `output_template`；
- `target_directory`、`temporary_directory`；
- `merge_output_format`（当前支持 mp4、mkv）；
- `DownloadExecutionOptions`：`rate_limit`、`retries`、`fragment_retries`、`file_access_retries`、`concurrent_fragments`。

创建时校验 URL、目标目录与任务元数据一致，格式 ID 非空且视频/音频 ID 不同，已提供的组合格式与快照一致，初始流和后续新增流的 format ID 与其媒体类型对应的快照 ID 一致。

快照与任务、初始流在同一事务中创建，没有公开更新接口；数据库触发器拒绝 UPDATE，删除任务时级联删除快照。

### 启动恢复与迁移

- `Storage::initialize()` 已自动执行异常恢复；初始化串行化，重复初始化不会再次改写活动任务。
- 遗留 `Preparing`、`Downloading`、`Merging` 任务统一转为 `Paused`，不自动启动下载。
- `DOWNLOAD_SCHEMA_VERSION = 2`。
- 迁移保留已有任务/流记录、ID、进度与生命周期时间。
- 保留 sqlite_sequence 高水位，避免重新使用已删除任务/流的 ID。
- 旧表删除后重建索引；外键数据校验在迁移提交前执行，失败回滚，退出迁移后恢复外键启用。
- 新 schema 禁止下载流处于 `Merging`；旧 schema 中此类流迁移为 `Paused` 并清除会话指标。
- 不为旧任务伪造执行快照；旧任务仍可读取、取消或删除，但缺快照时拒绝精确恢复。

## 3. 对外 API

| API | 职责与限制 |
| --- | --- |
| `create_download_task_with_execution_snapshot(draft, streams, snapshot)` | 原子创建可恢复任务及快照、初始流，返回 `DownloadTask` |
| `create_download_task(draft, streams)` | 保留旧兼容入口，不保存快照，因此不能用于需要继续能力的新执行路径 |
| `load_download_execution_snapshot(task_id)` | 返回 `Result<Option<DownloadExecutionSnapshot>, StorageError>`；任务不存在为 `DownloadNotFound`，任务存在但缺快照为 `Ok(None)` |
| `pause_download_task(task_id, now)` | 原子暂停准备中/下载中的任务与未结束流 |
| `prepare_resumed_download(task_id, now)` | 校验任务存在、Paused 和快照，原子准备继续；缺快照为 `DownloadExecutionSnapshotMissing` |
| `recover_interrupted_downloads(now)` | 返回转换的任务数，仅用于无执行器运行的启动阶段；initialize 已自动调用，不应在页面刷新或每次继续时调用 |

`now` 使用非负 Unix 秒。存储接口不控制 yt-dlp 子进程，不负责停止旧会话或隔离旧会话事件。恢复后晚到的旧进度必须由执行器阻止，不能仅靠数据库当前状态判断。

取消任务与取消流仍是现有独立接口；控制器须协调未结束流的终态处理，不应假定 `cancel_download_task()` 会隐式取消所有流。

## 4. 验证结果与限制

- `cargo fmt --check`：通过。
- 隔离存储验证：25 项 `storage_download_contract` 测试与 4 项 `storage_schema_contract` 测试通过。
- 启动恢复、全局恢复和故障注入用例在独立子进程执行，不与其他用例共享活动任务。
- 覆盖任务/流暂停恢复、进度拒绝、已完成流保留、终态不可恢复、缺快照拒绝、通用接口绕过拒绝、暂停/恢复/异常恢复的事务回滚、创建回滚、快照不可更新及级联删除、迁移外键回滚、索引与自增高水位。

本地临时验证工程位于 `target/storage-validation/`，直接通过路径引用真实 `src/storage/mod.rs`、真实 locale 源码和仓库的两个集成测试文件。没有复制或模拟存储实现，没有改动正式 Cargo 配置。运行命令：

```text
cargo test --manifest-path target/storage-validation/Cargo.toml --offline
```

该临时工程在 target 中，不作为正式发布文件；隔离验证不代表 GUI、下载控制器或整仓测试通过。

`cargo check` 当前失败于以下三个 E0004：

- `src/app/tasks.rs:114`；
- `src/app/tasks.rs:225`；
- `src/download_task/persistence.rs:167`。

解除这些编译阻塞后仍须执行：

```text
cargo fmt --check
cargo check
cargo test --test storage_download_contract
cargo test --test storage_schema_contract
cargo test
```

## 5. feature/tasks 修改意向

### 最小编译接入

- `task_status_key()` 增加 `DownloadTaskStatus::Paused => TextKey::TasksStatusPaused`。
- `status_rank()` 给 Paused 定义稳定排序位置，建议放在 Downloading 与 Merging 之间。
- 同步扩展独立任务页契约测试。

### 暂停/继续页面契约

- 准备中、下载中任务可发出暂停意向；暂停任务可发出继续意向。
- Merging 不可普通暂停，终态不可继续；缺执行快照的历史任务不得表现为可精确续传。
- 页面只发出 task_id，不重建 DownloadRequest，不直接写 SQLite 或管理子进程。
- 复核 `has_active_tasks()` 的语义；若用于运行中轮询，不应把 Paused 算作运行态。
- 继续成功后须主动刷新/重新读取状态，否则停止轮询的暂停列表可能一直不更新。装配层配合完成通知刷新。

## 6. feature/download_task 修改意向

### 最小编译接入

`src/download_task/persistence.rs` 的 `ensure_stream_downloading()` 显式拒绝 Paused，不可用通配分支静默放行，也不能自动将其转回 Preparing 或 Downloading。

### 完整执行器

1. 新建任务切换到 `create_download_task_with_execution_snapshot()`，在启动子进程前保存实际执行参数。
2. 提供按 task_id 暂停/继续的统一控制入口。页面不负责构造恢复请求。
3. 暂停与取消分开处理；暂停须保留续传文件、任务记录及已完成流，不能复用取消后重建任务。
4. 协调停止旧子进程、读取线程及回调，在允许继续前确认旧会话不再写入；采用会话代次或等效机制屏蔽晚到事件。
5. 继续时只从持久化快照取得请求级参数，调用专用恢复 API，复用原 task_id 和原流 ID；不得重新使用当前默认模板、目录或默认下载选项覆盖快照。
6. 保留累计进度与已完成流，处理已完成媒体、合并阶段异常退出后继续的场景。
7. 控制器保证同一任务不会因重复继续产生多个运行会话，并处理暂停/完成/失败竞争。
8. 进程重建失败时明确收敛状态，不留下永久 Preparing；定义当前工具路径、代理等环境配置读取策略，它们不属于上述请求级快照。
9. 校验续传目录和文件，覆盖真实 yt-dlp 续传、缺快照拒绝、旧会话事件、重复继续和进程启动失败。

## 7. feature/base 修改意向

- 在存储契约与下载控制器接口同步到基线后，装配页面暂停/继续意向和控制器。
- 不复用 `src/app/tasks_window.rs` 的 `redownload_task()`；该流程删除旧记录并新建任务，不能用于继续。
- `Storage::initialize()` 已含启动恢复，无需另加重复全局恢复调用；只需确保初始化成功后再装配运行入口。
- 在现有唯一 i18n 注入位置补上暂停状态 setter：

```rust
i18n.set_tasks_status_paused(snapshot.tasks_status_paused.into());
```

- 完成通知刷新、错误展示和生命周期管理，不持有其他页面的临时状态作为共享通道。
- 最后进行 Windows 真窗口和真实下载的集成验收。

## 8. feature/design-system 修改意向

当前基线 main 已包含 `TasksStatusPaused`、中英文“已暂停”/“Paused”、快照字段和 Slint 属性，不需要重复新增。

待页面操作契约明确后，如需暂停/继续按钮、执行失败或缺快照提示，再统一增加中英文 i18n 键与快照映射。不得在页面直接硬编码可见文案。新的控件状态样式继续使用 design.md 中登记的令牌。

## 9. 推荐协调顺序

1. 同步 storage 本轮接口变更。
2. tasks 与 download_task 各自完成最小 Paused 编译接入，协调合入基线，解除整体编译循环阻塞。
3. storage 基于整合基线重新执行正式编译和测试，完成整仓集成验证。
4. download_task 完成执行器；tasks 与 design-system 发布操作契约和文案。
5. base 最后装配并做真实窗口与续传验收。

其他分支不应自行修复或复制存储 schema，也不应把“新枚举能编译”当作“暂停/继续端到端完成”。
