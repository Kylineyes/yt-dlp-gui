# download_task 使用说明

`download_task` 是 yt-dlp 任务模块，当前提供：

- yt-dlp 文件版本验证；
- 视频 URL 异步元数据检索；
- 视频和媒体格式 JSON 解析；
- 任务取消；
- 可选任务超时；
- 下载业务后续所需的存放位置配置。

模块通过独立线程运行 yt-dlp，不阻塞调用线程。当前元数据检索使用 `--skip-download`，后续具体下载业务继续在本模块中扩展。

文档中的示例路径、代理和 URL 都是占位内容。示例 URL 使用 `.invalid` 保留域名：

```text
https://download-task.invalid/watch?v=metadata-fixture
```

该域名已通过 DNS 请求确认不可解析，不对应真实视频或服务。

## 一、理想业务路径

建议业务层按照下面顺序使用：

1. 从用户配置或界面取得 yt-dlp 路径、proxy、超时时间和存放位置。
2. 使用 `DownloadTaskClient::new` 创建客户端。
3. 使用 `verify_version` 检查 yt-dlp 是否可执行，并展示版本信息。
4. 用户提交 URL 后调用 `inspect_url`。
5. 立即保存返回的 `SearchHandle`，用于取消任务或等待结果。
6. 在 `MediaMessage::Metadata` 回调中读取 `VideoInfo`，更新页面中的标题、缩略图和格式列表。
7. 检索完成后，通过 `SearchHandle::wait` 获取最终结果。
8. 用户选择媒体格式后，后续下载业务使用 `storage_path` 作为保存位置，并基于 `MediaFormat` 组装下载参数。

### 理想路径示例

```rust
use std::time::Duration;
use yt_dlp_gui::download_task::{DownloadTaskClient, DownloadTaskError, MediaMessage};

fn inspect_video() -> Result<(), DownloadTaskError> {
    let client = DownloadTaskClient::new(
        "yt-dlp.exe",
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

GUI 接入时，回调运行在检索 worker 线程中，不能直接修改 Slint 控件；应把消息转发到 Slint 事件循环。GUI 线程也不应直接调用长时间阻塞的 `wait()`，可以在后台任务中等待后再转发结果。

## 二、逐个函数说明

### `DownloadTaskClient::new`

```rust
pub fn new(
    yt_dlp_path: impl Into<PathBuf>,
    proxy: Option<String>,
    timeout: Duration,
    storage_path: impl Into<PathBuf>,
) -> Self
```

创建 yt-dlp 客户端并保存任务配置。

| 参数 | 作用 |
| --- | --- |
| `yt_dlp_path` | yt-dlp 可执行文件路径，只保存，不在创建时验证 |
| `proxy` | 可选代理地址；空字符串或全空白字符串会被视为没有代理 |
| `timeout` | 元数据检索超时时间；零秒表示不设超时 |
| `storage_path` | 后续下载业务的存放位置，只保存，不检查路径是否存在 |

该函数不会启动 yt-dlp，也不会创建下载任务。

### `DownloadTaskClient::storage_path`

```rust
pub fn storage_path(&self) -> &Path
```

返回创建客户端时传入的存放位置。当前检索阶段不会访问、创建或验证该路径，后续下载业务可以使用它。

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

`--skip-download` 只针对当前元数据检索流程；后续下载业务可以在本模块中增加单独的调用方法。

### `SearchHandle::cancel`

```rust
pub fn cancel(&self)
```

请求取消当前任务。函数只设置取消标志，不等待 worker 线程结束。需要确认任务已经回收时，应继续调用 `wait()`。

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

等待任务完成，回收 worker 线程，并返回最终结果。

该函数会消耗 `SearchHandle`，同一个句柄只能调用一次 `wait()`。

- 成功时返回 `Ok(VideoInfo)`。
- 取消、超时、进程失败或解析失败时返回对应的 `DownloadTaskError`。

### `SearchHandle::drop`

`SearchHandle` 被丢弃时会自动发出取消请求，避免调用方忘记取消而留下后台 yt-dlp 进程。

## 三、逐个结构体和枚举说明

### `YtDlpVersion`

表示 yt-dlp 的版本信息。

| 字段 | 类型 | 作用 |
| --- | --- | --- |
| `value` | `String` | yt-dlp 输出的原始版本字符串 |

### `VideoInfo`

表示 yt-dlp 返回的单个视频元数据，是页面展示和后续下载业务的主要输入结构。

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

`id` 和 `title` 是必要字段；其他字段缺失时通常为 `None`。没有 `formats` 时返回空列表。

### `MediaFormat`

表示单个视频、音频或其他媒体格式的属性，是后续格式选择和下载参数构造的输入。

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

### `MediaMessage`

表示检索任务通过回调发送的状态消息。

| 枚举值 | 作用 |
| --- | --- |
| `Started` | worker 线程开始执行任务 |
| `Metadata(VideoInfo)` | JSON 已解析并得到视频元数据 |
| `Finished` | 检索成功结束 |
| `Cancelled` | 调用方主动取消任务 |
| `TimedOut` | 检索超过配置的非零超时时间 |

成功时的消息顺序：

```text
Started -> Metadata(VideoInfo) -> Finished
```

取消时通常为：

```text
Started -> Cancelled
```

超时时通常为：

```text
Started -> TimedOut
```

### `ClientConfig`

`ClientConfig` 是模块内部使用的配置结构，不对模块外公开。

| 字段 | 作用 |
| --- | --- |
| `yt_dlp_path` | yt-dlp 可执行文件路径 |
| `proxy` | 可选代理地址 |
| `timeout` | `Some(Duration)` 表示有超时，`None` 表示不设超时 |
| `storage_path` | 后续下载业务使用的存放位置，只保存不校验 |

## 四、异常情况

异常分为客户端创建阶段、版本验证阶段、任务执行阶段和结果解析阶段。

### 1. 客户端创建阶段

`DownloadTaskClient::new` 不执行路径校验，也不访问存放位置，因此：

- yt-dlp 路径不存在不会在创建时失败；
- 存放位置不存在不会在创建时失败；
- 存放位置没有权限不会在创建时失败；
- timeout 为零不会报错，而是表示不设超时；
- proxy 为 `None` 或空白字符串时不会传入 `--proxy`。

### 2. 版本验证异常

调用 `verify_version` 时可能出现：

| 错误 | 原因 |
| --- | --- |
| `ExecutableNotFound(path)` | yt-dlp 路径不存在，或路径不是普通文件 |
| `Spawn(error)` | 操作系统无法启动 yt-dlp |
| `VersionCommandFailed` | `--version` 返回非零退出码 |
| `VersionOutputEmpty` | yt-dlp 成功退出但没有输出版本字符串 |

### 3. URL 和 yt-dlp 进程异常

空字符串或无效 URL 不由 Rust 提前拦截，而是交给 yt-dlp：

```rust
let handle = client.inspect_url("", |_| {})?;
match handle.wait() {
    Err(DownloadTaskError::ProcessFailed { status, stderr }) => {
        println!("yt-dlp 退出码：{status:?}");
        println!("yt-dlp 错误：{stderr}");
    }
    result => println!("任务结果：{result:?}"),
}
```

可能出现：

| 错误 | 原因 |
| --- | --- |
| `ExecutableNotFound(path)` | worker 启动后发现 yt-dlp 文件不存在 |
| `Spawn(error)` | yt-dlp 进程无法启动 |
| `ProcessFailed` | yt-dlp 返回非零退出码，错误中包含退出码和 stderr |
| `Io(error)` | 读取 stdout/stderr 或回收进程失败 |
| `Cancelled` | 调用方请求取消 |
| `Timeout(timeout)` | 配置了非零 timeout，且任务超过该时间 |

### 4. JSON 解析异常

yt-dlp 成功退出后，模块会解析 stdout JSON：

| 错误 | 原因 |
| --- | --- |
| `InvalidJson(message)` | stdout 不是有效 JSON，或顶层 JSON 不是对象 |
| `MissingField(field)` | 缺少必要字段，例如 `id` 或 `title` |
| `InvalidField { field, message }` | 字段类型不正确，例如字符串字段返回数组 |

### 5. 任务状态异常

| 错误 | 原因 |
| --- | --- |
| `Poisoned` | 任务共享状态的互斥锁异常 |
| `WorkerPanicked` | worker 线程异常结束，无法正常提供结果 |

### 6. GUI 使用注意事项

- 回调运行在 worker 线程，不要直接修改 Slint 控件。
- 将回调消息转发到 Slint 事件循环后再更新 UI。
- 不要在 UI 线程直接调用长时间阻塞的 `wait()`。
- 取消按钮应保存 `SearchHandle`，点击时调用 `cancel()`。
- 收到 `Metadata` 后可以读取 `VideoInfo.formats` 展示格式列表。
- 后续下载按钮应使用 `storage_path` 和用户选择的 `MediaFormat` 构造下载任务。

## 五、测试

运行契约测试：

```text
cargo test --test download_task_contract
```

测试只使用占位路径和伪 URL，不包含真实本地路径、真实视频链接或真实视频 ID，也不依赖外部网络服务。
