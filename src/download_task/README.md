# download_task 使用说明

`download_task` 是 yt-dlp 任务模块，当前提供：

- yt-dlp 文件版本验证；
- 视频 URL 异步元数据检索；
- 视频和媒体格式 JSON 解析；
- 异步真实下载任务；
- yt-dlp 结构化下载进度解析；
- 多流进度聚合；
- 下载取消和超时；
- 从 Storage 环境配置读取 yt-dlp、FFmpeg、代理和默认下载目录；
- 创建下载任务与视频/音频流快照；
- 任务状态、节流进度和终态持久化；
- 下载完成后最终文件路径读取。

长时间运行的元数据检索和下载由独立线程执行。`download` 在返回句柄前会同步完成请求校验、yt-dlp 版本快照和 Storage 任务创建，因此调用方不应把这段短暂预处理放在对延迟极敏感的 UI 回调中。元数据检索使用 `--skip-download`，真实下载通过独立的 `DownloadTaskClient::download` 调用。

文档中的示例路径、代理和 URL 都是占位内容。示例 URL 使用 `.invalid` 保留域名：

```text
https://download-task.invalid/watch?v=metadata-fixture
```

该域名已通过 DNS 请求确认不可解析，不对应真实视频或服务。

## 一、理想业务路径

建议业务层按照下面顺序使用：

1. 完成 `Storage::initialize` 并保存环境配置。
2. 通常使用 `DownloadTaskClient::from_storage` 创建客户端；测试或特殊调用可使用 `new` 显式注入路径。
3. 使用 `verify_version` 检查 yt-dlp 是否可执行，并展示版本信息。
4. 用户提交 URL 后调用 `inspect_url`。
5. 立即保存返回的 `SearchHandle`，用于取消任务或等待结果。
6. 在 `MediaMessage::Metadata` 回调中读取 `VideoInfo`，更新页面中的标题、缩略图和格式列表。
7. 检索完成后，通过 `SearchHandle::wait` 获取最终结果。
8. 用户选择视频格式和音频格式后，构造 `DownloadRequest` 并调用 `download`。
9. 立即保存返回的 `DownloadHandle`，用于取消、读取实时进度或等待结果。
10. 在 `DownloadMessage` 回调中更新任务进度、合并状态和终态。
11. 完成后从 `DownloadResult::output_path` 读取最终文件路径。

### 元数据检索示例

```rust
use std::time::Duration;
use yt_dlp_gui::download_task::{DownloadTaskClient, DownloadTaskError, MediaMessage};

fn inspect_video() -> Result<(), DownloadTaskError> {
    let client = DownloadTaskClient::new(
        "yt-dlp.exe",
        Some("ffmpeg.exe".into()),
        Some("http://proxy.invalid:8080".to_owned()),
        Duration::from_secs(20),
        "download-output",
    );

    let version = client.verify_version()?;
    println!("yt-dlp 版本：{}", version.value);
    println!("保存位置：{}", client.storage_path().display());

    let handle = client.inspect_url(
        "https://download-task.invalid/watch?v=metadata-fixture",
        |message| match message {
            MediaMessage::Started => {
                println!("开始检索");
            }
            MediaMessage::Metadata(video) => {
                println!("标题：{}", video.title);
                println!("视频 ID：{}", video.id);
                println!("格式数量：{}", video.formats.len());
            }
            MediaMessage::Finished => {
                println!("检索完成");
            }
            MediaMessage::Cancelled => {
                println!("检索已取消");
            }
            MediaMessage::TimedOut => {
                println!("检索超时");
            }
        },
    )?;

    let video = handle.wait()?;
    println!("最终标题：{}", video.title);
    Ok(())
}
```

### 下载请求示例

```rust
use std::path::PathBuf;
use std::time::Duration;
use yt_dlp_gui::download_task::{
    DownloadMessage, DownloadOptions, DownloadRequest, DownloadTaskClient, VideoInfo,
};

fn start_download(client: &DownloadTaskClient, video: VideoInfo) -> Result<(), Box<dyn std::error::Error>> {
    let request = DownloadRequest {
        source_url: "https://download-task.invalid/watch?v=download-fixture".to_owned(),
        video,
        selected_video_format_id: "137".to_owned(),
        selected_audio_format_id: "251".to_owned(),
        output_template: "%(title)s.%(ext)s".to_owned(),
        target_directory: PathBuf::from("download-output"),
        temporary_directory: PathBuf::from("download-temp"),
        merge_output_format: "mp4".to_owned(),
        options: DownloadOptions {
            rate_limit: Some("2M".to_owned()),
            retries: Some(3),
            fragment_retries: Some(5),
            file_access_retries: Some(3),
            concurrent_fragments: Some(4),
        },
    };

    let handle = client.download(request, |message| match message {
        DownloadMessage::Started => println!("开始下载"),
        DownloadMessage::StreamProgress(stream) => {
            println!(
                "流 {} 下载中：{:?}，速度 {:?}",
                stream.stream_key,
                stream.media_type,
                stream.speed_bytes_per_second
            );
        }
        DownloadMessage::Progress(progress) => {
            println!("任务进度：{:?}", progress.percent);
        }
        DownloadMessage::Merging => {
            println!("开始合并");
        }
        DownloadMessage::Completed(result) => {
            println!("下载完成：{:?}", result.output_path);
        }
        DownloadMessage::Cancelled => {
            println!("下载已取消");
        }
        DownloadMessage::Failed(error) => {
            println!("下载失败：{error}");
        }
    })?;
    handle.wait()?;
    Ok(())
}
```

GUI 接入时，回调运行在 worker 线程中，不能直接修改 Slint 控件；应把消息转发到 Slint 事件循环。GUI 线程也不应直接调用长时间阻塞的 `wait()`，可以在后台任务中等待后再转发结果。

## 二、逐个函数说明

### `DownloadTaskClient::new`

```rust
pub fn new(
    yt_dlp_path: impl Into<PathBuf>,
    ffmpeg_path: Option<PathBuf>,
    proxy: Option<String>,
    timeout: Duration,
    storage_path: impl Into<PathBuf>,
) -> Self
```

创建 yt-dlp 客户端并保存任务配置。

| 参数 | 作用 |
| --- | --- |
| `yt_dlp_path` | yt-dlp 可执行文件路径，只保存，不在创建时验证 |
| `ffmpeg_path` | 可选 FFmpeg 可执行文件或 bin 目录；下载时通过 `--ffmpeg-location` 传给 yt-dlp |
| `proxy` | 可选代理地址；空字符串或全空白字符串会被视为没有代理 |
| `timeout` | 元数据检索和下载任务的超时时间；零秒表示不设超时 |
| `storage_path` | 后续下载业务的存放位置，只保存，不检查路径是否存在 |

该函数不会启动 yt-dlp，也不会创建下载任务。

### `DownloadTaskClient::from_storage`

```rust
pub fn from_storage(timeout: Duration) -> Result<Self, DownloadTaskError>
```

从已初始化的 `Storage` 环境配置读取 yt-dlp、FFmpeg、代理和默认下载目录。配置为空或 Storage 尚未初始化时返回 `DownloadTaskError::Storage`。这是应用层推荐使用的构造方式。

### `DownloadTaskClient::storage_path`

```rust
pub fn storage_path(&self) -> &Path
```

返回创建客户端时传入的存放位置。检索阶段不会访问、创建或验证该路径；真实下载应显式传入 `DownloadRequest::target_directory`。

### `DownloadTaskClient::verify_version`

```rust
pub fn verify_version(&self) -> Result<YtDlpVersion, DownloadTaskError>
```

执行以下命令验证 yt-dlp：

```text
<yt-dlp-path> --version
```

成功时返回 `YtDlpVersion`。版本号按字符串保存，不强制转换为 SemVer，因为 yt-dlp 常使用日期形式或带后缀的版本。

### `DownloadTaskClient::inspect_url`

```rust
pub fn inspect_url<F>(
    &self,
    url: impl Into<String>,
    on_message: F,
) -> Result<SearchHandle, DownloadTaskError>
where
    F: Fn(MediaMessage) + Send + 'static
```

启动异步元数据检索，并立即返回 `SearchHandle`。

| 参数 | 作用 |
| --- | --- |
| `url` | 传给 yt-dlp 的 URL，包括空字符串；空值由 yt-dlp 自己返回错误 |
| `on_message` | 接收任务状态和成功元数据的回调 |

当前检索参数为：

```text
--dump-single-json
--skip-download
--no-warnings
--no-playlist
[--proxy <proxy>]
-- <url>
```

`--skip-download` 只针对当前元数据检索流程；真实下载使用 `DownloadTaskClient::download`。

### `DownloadTaskClient::download`

```rust
pub fn download<F>(
    &self,
    request: DownloadRequest,
    on_message: F,
) -> Result<DownloadHandle, DownloadTaskError>
where
    F: Fn(DownloadMessage) + Send + 'static
```

启动异步下载任务，并立即返回 `DownloadHandle`。

`download` 会先验证请求、检查 yt-dlp 版本，并通过当前 `Storage` 单例创建任务及两个初始流。空字段、找不到所选格式、流类型不匹配或非 `mp4`/`mkv` 的合并容器会返回 `InvalidDownloadRequest`，不会启动下载子进程。任务 ID 由数据库生成，可通过 `DownloadHandle::task_id()` 读取。

当前下载参数为：

```text
--check-formats
--newline
--progress
--progress-delta 0.5
--progress-template download:download:<download-template>
--progress-template postprocess:postprocess:<postprocess-template>
--print after_move:after_move:%(filepath)s
--no-simulate
--paths home:<target-directory>
--paths temp:<temporary-directory>
--output <output-template>
--continue
--no-overwrites
--part
--merge-output-format <mp4|mkv>
[--proxy <proxy>]
[--ffmpeg-location <ffmpeg-path>]
[--limit-rate <rate-limit>]
[--retries <n>]
[--fragment-retries <n>]
[--file-access-retries <n>]
[--concurrent-fragments <n>]
-f <video-format-id>+<audio-format-id>
-- <source-url>
```

说明：

- `--no-simulate` 确保输出 `after_move` 路径时仍执行真实下载；
- `--print` 会隐式启用 quiet，因此显式增加 `--progress` 恢复结构化下载进度；
- 模板中的第一个 `download:` / `postprocess:` / `after_move:` 是类型或时机选择器，第二个同名前缀才是实际输出文本；
- `--check-formats` 在下载前检查所选格式；
- `--continue`、`--no-overwrites` 和 `--part` 支持断点续传并防止覆盖；
- `--progress-delta 0.5` 限制进度输出频率；
- `--newline` 保证每行是一个独立进度记录；
- 代理、限速、重试和分片并发参数只在使用可选配置时传入；
- 模块不使用 shell 拼接命令，避免参数注入。

### `SearchHandle::cancel`

```rust
pub fn cancel(&self)
```

请求取消当前元数据检索任务。函数只设置取消标志，不等待 worker 线程结束。需要确认任务已经回收时，应继续调用 `wait()`。

重复调用是安全的，不会重复终止进程。

### `SearchHandle::is_cancelled`

```rust
pub fn is_cancelled(&self) -> bool
```

返回是否已经发出取消请求。

返回 `true` 只表示取消请求已发出，不代表 yt-dlp 子进程已经完成终止和回收。

### `SearchHandle::latest_result`

```rust
pub fn latest_result(&self) -> Option<VideoInfo>
```

读取当前已经成功解析的视频结果。

- 成功解析并发送 `MediaMessage::Metadata` 后返回 `Some(VideoInfo)`。
- 任务尚未结束时返回 `None`。
- 任务失败、取消或超时时返回 `None`。
- 返回结果是副本，不会暴露内部锁。

### `SearchHandle::wait`

```rust
pub fn wait(self) -> Result<VideoInfo, DownloadTaskError>
```

等待元数据检索完成，回收 worker 线程，并返回最终结果。

该函数会消耗 `SearchHandle`，同一个句柄只能调用一次 `wait()`。

- 成功时返回 `Ok(VideoInfo)`。
- 取消、超时、进程失败或解析失败时返回对应的 `DownloadTaskError`。

### `DownloadHandle::task_id`

```rust
pub fn task_id(&self) -> i64
```

返回 Storage 创建的下载任务 ID，可用于任务页读取数据库快照。

### `DownloadHandle::cancel`

```rust
pub fn cancel(&self)
```

请求取消当前下载任务。worker 会终止 yt-dlp 子进程，并在收尾后报告 `Cancelled`。

### `DownloadHandle::is_cancelled`

```rust
pub fn is_cancelled(&self) -> bool
```

返回是否已经发出取消请求。

### `DownloadHandle::latest_progress`

```rust
pub fn latest_progress(&self) -> Option<DownloadProgress>
```

读取最近一次聚合后的任务级进度快照。尚未收到任何进度时返回 `None`。

### `DownloadHandle::wait`

```rust
pub fn wait(self) -> Result<DownloadResult, DownloadTaskError>
```

等待下载完成，回收 worker 线程，并返回最终结果。

该函数会消耗 `DownloadHandle`，同一个句柄只能调用一次 `wait()`。

- 成功时返回 `Ok(DownloadResult)`。
- 取消、超时、进程失败或解析失败时返回对应的 `DownloadTaskError`。

### `DownloadHandle::drop`

`DownloadHandle` 被丢弃时会自动发出取消请求，避免调用方忘记取消而留下后台 yt-dlp 进程。

### 纯函数

```rust
pub fn parse_download_progress_line(
    line: &str,
    video_format_id: &str,
    audio_format_id: &str,
) -> Result<Option<StreamProgress>, DownloadTaskError>

pub fn aggregate_download_progress(
    task_id: i64,
    streams: &[StreamProgress],
    updated_at: i64,
) -> DownloadProgress
```

`parse_download_progress_line` 解析单行下载进度：

- `download:` 行返回 `Some(StreamProgress)`；
- `postprocess:` 和 `after_move:` 行返回 `None`；
- 无效字段、未知格式 ID 或未知状态返回 `ProgressParse`。

`aggregate_download_progress` 汇总多个流进度。准确 `total_bytes` 优先；任一流缺少准确总量但存在估算总量时使用估算值并标记 `total_is_estimate`；没有任何可靠总大小时，百分比和 ETA 保持 `None`。

## 三、逐个结构体和枚举说明

### `YtDlpVersion`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `value` | `String` | yt-dlp 输出的原始版本字符串 |

### `VideoInfo`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `id` | `String` | 视频在站点上的唯一标识 |
| `title` | `String` | 视频标题 |
| `webpage_url` | `Option<String>` | 视频规范页面地址 |
| `original_url` | `Option<String>` | 发起检索时传入的原始地址 |
| `uploader` | `Option<String>` | 发布者或上传者名称 |
| `channel` | `Option<String>` | 所属频道名称 |
| `duration_seconds` | `Option<f64>` | 视频时长，单位为秒 |
| `thumbnail_url` | `Option<String>` | 视频缩略图地址 |
| `description` | `Option<String>` | 视频描述文本 |
| `upload_date` | `Option<String>` | 发布日期，通常为 `YYYYMMDD` |
| `formats` | `Vec<MediaFormat>` | 可用媒体格式列表 |

### `MediaFormat`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `format_id` | `Option<String>` | yt-dlp 分配的格式标识 |
| `format_note` | `Option<String>` | 格式补充说明 |
| `extension` | `Option<String>` | 媒体文件扩展名 |
| `resolution` | `Option<String>` | 面向用户展示的分辨率文本 |
| `width` | `Option<u64>` | 视频宽度，单位为像素 |
| `height` | `Option<u64>` | 视频高度，单位为像素 |
| `fps` | `Option<f64>` | 视频帧率 |
| `filesize` | `Option<u64>` | 精确文件大小，单位为字节 |
| `filesize_approx` | `Option<u64>` | 估算文件大小，单位为字节 |
| `bitrate_kbps` | `Option<f64>` | 综合码率，单位为 Kbps |
| `video_codec` | `Option<String>` | 视频编码器名称 |
| `audio_codec` | `Option<String>` | 音频编码器名称 |
| `audio_bitrate_kbps` | `Option<f64>` | 音频码率，单位为 Kbps |
| `video_bitrate_kbps` | `Option<f64>` | 视频码率，单位为 Kbps |
| `protocol` | `Option<String>` | yt-dlp 使用的传输协议 |
| `url` | `Option<String>` | 当前格式的媒体地址，可能是临时地址 |

### `DownloadRequest`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `source_url` | `String` | 要下载的 URL |
| `video` | `VideoInfo` | 已解析的视频元数据 |
| `selected_video_format_id` | `String` | 视频流格式 ID |
| `selected_audio_format_id` | `String` | 音频流格式 ID |
| `output_template` | `String` | yt-dlp `--output` 文件名模板 |
| `target_directory` | `PathBuf` | `--paths home` 最终目录 |
| `temporary_directory` | `PathBuf` | `--paths temp` 临时目录 |
| `merge_output_format` | `String` | 仅支持 `mp4` 或 `mkv` |
| `options` | `DownloadOptions` | 可选下载参数 |

### `DownloadOptions`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `rate_limit` | `Option<String>` | `--limit-rate` 值，例如 `2M` |
| `retries` | `Option<u32>` | `--retries` |
| `fragment_retries` | `Option<u32>` | `--fragment-retries` |
| `file_access_retries` | `Option<u32>` | `--file-access-retries` |
| `concurrent_fragments` | `Option<u32>` | `--concurrent-fragments` |

### `StreamProgress`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `stream_key` | `String` | 流标识，通常为格式 ID |
| `format_id` | `Option<String>` | 解析得到的格式 ID |
| `media_type` | `DownloadMediaType` | `Video` 或 `Audio` |
| `status` | `DownloadStreamStatus` | `Downloading` 或 `Finished` |
| `downloaded_bytes` | `i64` | 已下载字节数，缺失时按 0 处理 |
| `total_bytes` | `Option<i64>` | 准确总大小 |
| `total_bytes_estimate` | `Option<i64>` | 估算总大小 |
| `speed_bytes_per_second` | `Option<i64>` | 实时速度 |
| `elapsed_seconds` | `Option<i64>` | 已用秒数 |
| `eta_seconds` | `Option<i64>` | 预计剩余秒数 |
| `percent` | `Option<u8>` | 0 到 100 的百分比 |
| `started_at` | `Option<i64>` | 流开始时间戳 |
| `finished_at` | `Option<i64>` | 流结束时间戳 |

### `DownloadProgress`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `task_id` | `i64` | 任务标识 |
| `stage` | `DownloadStage` | 当前任务阶段 |
| `downloaded_bytes` | `i64` | 各流已下载字节数之和 |
| `total_bytes` | `Option<i64>` | 各流准确总大小之和 |
| `total_bytes_estimate` | `Option<i64>` | 含估算值时的总大小 |
| `speed_bytes_per_second` | `Option<i64>` | 各流速度之和 |
| `elapsed_seconds` | `Option<i64>` | 各流耗时最大值 |
| `eta_seconds` | `Option<i64>` | 基于剩余字节和速度的预计秒数 |
| `percent` | `Option<u8>` | 聚合百分比，范围为 0 到 100 |
| `total_is_estimate` | `bool` | 总大小是否包含估算来源 |
| `active_stream` | `Option<String>` | 当前仍在下载的流标识 |
| `updated_at` | `i64` | 聚合时间戳 |

### `DownloadResult`

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `task_id` | `i64` | 任务标识 |
| `output_path` | `Option<PathBuf>` | yt-dlp 通过 `after_move` 报告的最终文件路径 |

### `MediaMessage`

| 枚举值 | 作用 |
| --- | --- |
| `Started` | 检索 worker 开始执行 |
| `Metadata(VideoInfo)` | JSON 已解析并得到视频元数据 |
| `Finished` | 检索成功结束 |
| `Cancelled` | 检索已取消 |
| `TimedOut` | 检索超时 |

成功顺序：

```text
Started -> Metadata(VideoInfo) -> Finished
```

### `DownloadMessage`

| 枚举值 | 作用 |
| --- | --- |
| `Started` | 下载 worker 开始执行 |
| `StreamProgress(StreamProgress)` | 单个流的实时进度 |
| `Progress(DownloadProgress)` | 多流聚合后的任务进度 |
| `Merging` | 进入合并阶段 |
| `Completed(DownloadResult)` | 下载成功并得到最终结果 |
| `Cancelled` | 下载被取消 |
| `Failed(DownloadTaskError)` | 下载失败 |

下载阶段的消息顺序通常为：

```text
Started
StreamProgress(...)
Progress(...)
StreamProgress(...)
Progress(...)
Merging
Completed(DownloadResult)
```

取消和失败消息均为终态，收到后不应再期待后续消息。

### `DownloadMediaType` / `DownloadStreamStatus` / `DownloadStage`

```text
DownloadMediaType: Video | Audio
DownloadStreamStatus: Downloading | Finished
DownloadStage: Preparing | Downloading | Merging | Completed
```

当前模块在收到下载进度时使用 `Downloading`，收到 `postprocess` 行时报告 `Merging`。

## 四、进度模板和聚合规则

模块使用两个进度模板：

- 下载模板输出 `download:` 开头的九字段制表符分隔行；
- 后处理模板输出 `postprocess:` 开头的阶段行；
- `--print after_move:after_move:%(filepath)s` 输出 `after_move:` 最终路径行。

`NA`、`N/A`、`none` 和空值统一解析为 `Option::None`，不会保存为字符串。

多流聚合规则：

- 已下载字节数为各流之和；
- 准确总大小优先，缺少准确总大小时使用估算值；
- 任一流缺少准确总大小但使用估算值时，`total_is_estimate` 为 `true`；
- 没有任何可靠总大小时，百分比和 ETA 为 `None`；
- 合并阶段不伪造合并百分比。

### 持久化策略

- 调用 `download` 时原子创建主任务和视频/音频初始流，并立即进入 `preparing`；
- worker 启动后立即迁移到 `downloading`；
- 首次收到后处理事件时立即迁移到 `merging`；
- 任务进度和当前流进度最多每 750ms 写入一次，流完成时强制写入；
- 进度写入失败不会改变实际下载成功判断；
- 完成、取消和失败终态立即写入，最终路径和受限错误摘要一并保存；
- Storage 当前没有流级状态迁移 API，因此模块保存流进度，但不能把数据库中的单流状态迁移到 `downloading`/`completed`；该契约必须由 Storage 负责分支提供后才能接入。

## 五、异常情况

异常分为客户端创建阶段、版本验证阶段、任务执行阶段、下载阶段和结果解析阶段。

### 1. 客户端创建阶段

`DownloadTaskClient::new` 不执行路径校验，也不访问存放位置，因此：

- yt-dlp 路径不存在不会在创建时失败；
- 存放位置不存在不会在创建时失败；
- timeout 为零不会报错，而是表示不设超时；
- proxy 为 `None` 或空白字符串时不会传入 `--proxy`。

### 2. 版本验证异常

| 错误 | 原因 |
| --- | --- |
| `ExecutableNotFound(path)` | yt-dlp 路径不存在，或路径不是普通文件 |
| `Spawn(error)` | 操作系统无法启动 yt-dlp |
| `VersionCommandFailed` | `--version` 返回非零退出码 |
| `VersionOutputEmpty` | yt-dlp 成功退出但没有输出版本字符串 |

### 3. 元数据检索异常

| 错误 | 原因 |
| --- | --- |
| `ProcessFailed` | yt-dlp 返回非零退出码，错误包含退出码和 stderr |
| `Io(error)` | 读取 stdout/stderr 或回收进程失败 |
| `Cancelled` | 调用方请求取消 |
| `Timeout(timeout)` | 配置了非零 timeout，且任务超过该时间 |
| `InvalidJson(message)` | stdout 不是有效 JSON |
| `MissingField(field)` | 缺少必要字段，例如 `id` 或 `title` |
| `InvalidField { field, message }` | 字段类型不正确 |

### 4. 下载异常

| 错误 | 原因 |
| --- | --- |
| `InvalidDownloadRequest(message)` | 下载请求为空、格式不存在、媒体类型不匹配或合并容器不是 `mp4`/`mkv` |
| `ExecutableNotFound(path)` | worker 启动后未找到 yt-dlp |
| `Spawn(error)` | 下载进程无法启动 |
| `ProgressParse(message)` | 进度行字段数量、数值、百分比、速度单位或格式 ID 无法解析 |
| `DownloadProcessFailed { status, stderr }` | yt-dlp 下载返回非零退出码 |
| `Timeout(timeout)` | 下载超过客户端配置的非零超时时间 |
| `Cancelled` | 调用方取消下载 |
| `Storage(message)` | Storage 未初始化、配置缺失或任务状态/终态写入失败 |
| `OutputPathMissing` | yt-dlp 成功退出但没有返回 `after_move` 最终路径 |

下载失败保存的是受限的 stderr 摘要，不保存完整命令行、临时媒体 URL 或 proxy 认证信息。

### 5. 任务状态异常

| 错误 | 原因 |
| --- | --- |
| `Poisoned` | 共享状态的互斥锁异常 |
| `WorkerPanicked` | worker 线程异常结束 |

### 6. GUI 使用注意事项

- 回调运行在 worker 线程，不要直接修改 Slint 控件。
- 将回调消息转发到 Slint 事件循环后再更新 UI。
- 不要在 UI 线程直接调用长时间阻塞的 `wait()`。
- 取消按钮应保存 `DownloadHandle`，点击时调用 `cancel()`。
- 收到 `Progress` 后可更新任务进度条；收到 `StreamProgress` 后可更新单流详情。
- 任务终态（`Completed`、`Cancelled`、`Failed`）后应停止接收更多消息。

## 六、测试

运行自动化契约测试：

```text
cargo test --test download_task_contract
cargo test --test download_task_worker_contract
```

测试覆盖：

- 元数据检索默认超时、缺失可执行文件和取消；
- 结构化下载进度 fixture 解析及多流汇总；
- fake yt-dlp 真实子进程启动和参数转发；
- Storage 任务/流创建、进度、合并、完成、失败、超时和取消终态；
- FFmpeg 路径、代理、格式组合、目标/临时目录和 `--no-simulate` 参数。

真实下载验收默认忽略，通过环境变量显式启用：

```text
YTDLP_GUI_TEST_YT_DLP=<yt-dlp-path>
YTDLP_GUI_TEST_FFMPEG=<ffmpeg-path>
YTDLP_GUI_TEST_URL=<video-url>
YTDLP_GUI_TEST_PROXY=<optional-proxy>
cargo test --test download_task_real_acceptance -- --ignored --nocapture
```

自动化测试只使用占位路径和伪 URL，不依赖外部网络。真实验收不把具体 URL、代理、下载目录或视频文件写入仓库。
