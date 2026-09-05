# 下载执行器：单任务暂停与继续（首轮）

## 基线与范围

`feature/download_task` 已 rebase 到 `origin/feature/storage` 的 `292b9a9`，接入根目录 `storage-pause-resume.md` 中已发布的存储 API。

本轮仅修改 `src/download_task/` 和独立下载测试，不修改存储 schema、任务页面、共享文案或窗口装配。执行器基础路径已可用，但不代表所有恢复边界或 GUI 端到端功能已经完成。

## 数据与不变量

- 创建下载前将目标目录和临时目录固定为绝对目录，原子保存任务、两条初始流和 `DownloadExecutionSnapshot`，然后启动执行会话。
- 请求级参数只能从不可变快照恢复，包括 URL、视频/音频格式、模板、目录、合并格式和下载选项；不重新创建任务或流。
- 环境级参数采用当前 `DownloadTaskClient` 的工具路径、代理和超时。装配层需要采用最新已保存配置时，应重新调用 `from_storage()`，而不是传递其他页面的临时状态。
- 每个 task_id 在进程内只允许登记一个会话。登记从恢复验证开始，持续到子进程、输出读取线程及同步回调全部退出。
- 暂停与取消是不同的停止原因，先接受的停止意向生效。进入合并或收尾阶段后拒绝新的停止请求。
- 暂停在停止进程、丢弃未处理的旧输出并等待读取线程结束后，刷新最后已接收的累计进度，再调用专用存储暂停 API。
- 暂停不删除 `.part`、`.ytdl` 或已下载媒体，不把已完成流改回活动态；存储层清除速度、ETA 和会话耗时，保留累计进度及首次开始时间。
- 继续以持久化进度作为初始进度，旧会话退出前拒绝继续。使用 `--ignore-config` 隔离用户级 yt-dlp 配置，保留 `--continue`、`--part` 和 `--no-overwrites`。
- 缺快照、目录不存在、非暂停状态或工具预检失败时，拒绝恢复并释放会话登记；专用恢复 API 成功后发生的启动错误或工作线程异常会收敛为失败，而不遗留 Preparing。

## 接口

| 接口 | 语义 |
| --- | --- |
| `DownloadTaskClient::pause_download(task_id)` | 同步暂停活动任务；成功返回前等待旧会话退出，不应在 GUI 线程调用 |
| `DownloadTaskClient::request_pause_download(task_id)` | 仅提交暂停请求，允许从同步下载回调内调用；返回成功不代表已经暂停 |
| `DownloadTaskClient::resume_download(task_id, on_message)` | 从数据库恢复请求并启动新会话，复用原任务及流 ID |
| `DownloadHandle::request_pause()` | 非阻塞暂停意向 |
| `DownloadHandle::pause()` | 同步暂停；在本会话回调线程调用时明确拒绝，防止自等待 |
| `DownloadHandle::wait_outcome()` | 等待会话及回调退出，返回 `DownloadOutcome::Completed(result)` 或 `DownloadOutcome::Paused { task_id }`；失败与取消仍通过 `Err` 返回 |
| `DownloadHandle::wait()` | 兼容原有只等待完整下载的调用方；暂停时返回明确的 `InvalidDownloadRequest` 提示，不伪造下载完成结果 |

暂停状态通过 `DownloadMessage::Progress` 中的 `DownloadStage::Paused` 发出，不发送 `Completed`、`Cancelled` 或 `Failed` 消息。该进度通知仍发生在工作线程中；需要立即继续时，先在装配层等待 `pause_download()` 或 `wait_outcome()` 完成，不要在旧会话回调内直接继续。

## 当前验证

- `cargo fmt --check`：通过。
- 隔离回归：8 项下载解析/检索契约、1 项下载工作线程契约、1 项暂停继续组合契约、25 项存储下载契约、4 项存储 schema 契约，共 39 项通过。
- 暂停继续组合契约覆盖：累计进度/首次时间/ID 保留、已完成视频流完整保留、旧回调尚未退出时拒绝继续、环境配置更新而请求快照不变、缺快照、目录失效、工具预检失败、合并阶段拒绝暂停、恢复启动失败及初始回调 panic。
- 真实 yt-dlp 本地 HTTP 验收：使用 yt-dlp `2026.08.19` 与 FFmpeg `9.0.1`，暂停时累计 64,512 字节，保留续传文件；继续发出非零偏移 Range 请求，复用原记录，输出媒体可由 FFmpeg 完整解码。
- 整仓 `cargo check` 与 `cargo test` 均失败（退出码 101），被 `src/app/tasks.rs:114`、`:225` 的 `DownloadTaskStatus::Paused` 穷尽匹配阻塞。本分支自己的 persistence 匹配已接入，未跨分支修复任务页。

临时隔离工程位于 `target/download-task-validation/`，直接通过路径引用真实下载、存储及 Locale 源文件，复用根目录的独立测试文件，不复制生产实现，不修改正式 Cargo 配置。该临时工程不受 Git 跟踪，不能替代整仓集成验证：

```text
cargo test --manifest-path target/download-task-validation/Cargo.toml --offline
```

整合编译阻塞解除后应运行正式命令：

```text
cargo check
cargo test --test download_task_contract
cargo test --test download_task_worker_contract
cargo test --test download_task_pause_resume_contract
cargo test
```

真实验收默认忽略，需要显式提供真实工具路径；验收自行生成短媒体并启动本机 HTTP 服务，无须外部网站 URL：

```powershell
$env:YTDLP_GUI_TEST_YT_DLP = (Get-Command yt-dlp).Source
$env:YTDLP_GUI_TEST_FFMPEG = (Get-Command ffmpeg).Source
cargo test --test download_task_real_pause_resume -- --ignored --nocapture
```

本轮在临时工程上运行同名真实测试，整仓恢复编译后仍应重跑正式入口。

## 尚待补齐的执行器边界

1. 当前恢复前校验目录存在且可枚举，但未建立每个续传文件的精确路径/完整性清单。临时文件被外部删除、同目录同模板冲突、远端标题变化导致模板展开不同的情况，仍需补充明确的文件级验证策略。
2. 本轮已验证正常下载中断与已完成流记录保留；“进程在合并阶段异常退出、最终输出文件已经存在或损坏、yt-dlp 跳过所有下载钩子”的组合尚未完成执行器级验收，不能仅凭存储启动恢复通过就宣称支持。
3. Windows 停止路径使用系统 `taskkill /T /F` 结束进程树，并等待读取线程；仍需补充外部下载器/FFmpeg 子进程的专门故障注入测试。Windows 10 1903+ 真机兼容性尚未单独验收。

## 其他分支接入计划

- `feature/tasks`：补齐 Paused 文案映射和排序，发布仅携带 task_id 的暂停/继续意向；历史缺快照任务不可表现为可精确继续。
- `feature/base`：装配控制入口，在非 GUI 线程执行阻塞操作；采用 `wait_outcome()` 区分暂停和完成，并在操作完成后重新读取任务列表。继续不得复用删除旧记录的 `redownload_task()`。
- `feature/base` / `feature/design-system`：在唯一 i18n 注入位置接入已发布的暂停状态文案，按页面契约补齐操作文案。
- 完成执行器边界和上述集成后，再做 Windows 真窗口验收及正式整仓测试。
