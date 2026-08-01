# Storage initialization integration

本文档说明应用启动流程如何对接 SQLite 存储初始化，以及后续前端分支如何处理致命的初始化失败。

## 对接接口

应用 worker 的启动入口是：

```rust
Storage::open_default() -> Result<Storage, AppError>
```

定义位置：`src/storage.rs`。

该接口会：

1. 通过 `std::env::current_exe()` 获取可执行文件位置；
2. 使用 `<exe-directory>/application.sqlite3` 作为数据库路径；
3. 创建缺失的数据库父目录；
4. 打开现有数据库或创建新数据库；
5. 配置 SQLite 连接；
6. 创建 `app_config`、`downloads` 和 `download_logs`；
7. 执行兼容旧数据库所需的字段迁移、索引创建和 schema 版本更新；
8. 仅在全部步骤成功后返回可用的 `Storage`。

调用方不要先执行 `Path::exists()`。该接口本身是幂等的，并且即使数据库文件已经存在，也会确保 schema 初始化完整。

如测试或后续功能需要指定数据库路径，可调用：

```rust
Storage::open_or_initialize(path) -> Result<Storage, AppError>
```

## 推荐调用时机

worker 应在发送 `WorkerEvent::Ready` 和进入普通命令循环之前调用：

```rust
let storage = match Storage::open_default() {
    Ok(storage) => storage,
    Err(error) => {
        // Send a dedicated fatal initialization event here.
        return;
    }
};
```

初始化失败后不得继续发送 `Ready`，也不得进入依赖数据库的命令循环。

## 错误契约

失败时返回以下结构化错误之一：

- `AppError::StorageIo`
- `AppError::StorageSqlite`

它们携带：

- `StorageStage`：失败阶段；
- 实际文件或目录路径（能够取得时）；
- 底层 I/O 或 SQLite 错误。

当前阶段包括：

- `ResolveExecutablePath`
- `CreateDatabaseDirectory`
- `OpenDatabase`
- `ConfigureConnection`
- `CreateTables`
- `MigrateSchema`

`error.to_string()` 可直接作为技术详情展示，例如：

```text
Failed to open or create the SQLite database at 'C:\Program Files\yt-dlp-gui\application.sqlite3': access denied
```

`std::error::Error::source()` 可取得底层错误链。

前端不得通过解析 `error.to_string()` 来判断是否属于启动初始化失败；应由 worker 在调用边界使用专用事件区分。

## 后续前端分支的事件建议

建议新增专用事件：

```rust
pub enum WorkerEvent {
    StorageInitializationFailed { detail: String },
    // Existing events...
}
```

worker 对接示例：

```rust
let storage = match Storage::open_default() {
    Ok(storage) => storage,
    Err(error) => {
        let _ = events.send(WorkerEvent::StorageInitializationFailed {
            detail: error.to_string(),
        });
        return;
    }
};
```

不要将初始化失败继续作为普通 `WorkerEvent::Error` 写入设置、检索和下载三个页面的 `message-argument`。普通运行期间的保存、查询和下载记录错误仍可沿用现有 `WorkerEvent::Error`。

## 后续模态窗口行为

收到 `StorageInitializationFailed` 后，前端分支应：

1. 在 `MainWindow` 根层显示阻塞式模态窗口，不依赖当前选中的页面；
2. 提示应用存储文件创建或初始化失败；
3. 提示用户检查可执行文件所在目录是否异常，以及是否存在文件权限问题；
4. 展示事件中的 `detail` 技术详情；
5. 只提供一个“确定”按钮，不提供取消或重试；
6. 点击遮罩不关闭模态，并阻止操作底层页面；
7. 点击“确定”时调用 `slint::quit_event_loop()`；
8. 让 `window.run()` 返回后复用现有 `UiCommand::Shutdown` 和 worker `join` 收尾逻辑。

所有 UI 文案必须使用 Slint `@tr(...)` 的英文 source，并在 `translations/zh-CN/LC_MESSAGES/yt-dlp-gui.po` 添加中文翻译。动态 `detail` 应作为占位符参数传入，不要在 Rust 中拼接本地化句子。

建议后续修改文件：

- `src/app/mod.rs`：新增专用事件并发送；
- `src/main.rs`：接收事件、显示模态、处理确定退出；
- `src/ui.slint`：模态状态、遮罩、内容和唯一按钮；
- `translations/zh-CN/LC_MESSAGES/yt-dlp-gui.po`：模态文案翻译。

本存储分支不实现上述前端调度。
