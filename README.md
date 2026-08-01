# yt-dlp-gui

使用 Rust 和 Slint 构建的 `yt-dlp` 桌面图形化前端。

## 开发命令

```bash
cargo check
cargo run
cargo fmt --all -- --check
cargo test --all
cargo clippy --all-targets --all-features -- -D warnings
```

## 外部依赖

应用通过 `ytd-rs` 启动外部 `yt-dlp` 程序。运行下载前，请确保 `yt-dlp` 已加入 `PATH`，或者在界面中填写 `yt-dlp.exe` 的绝对路径。需要合并音视频或执行后处理时，还需要安装 FFmpeg。

## 数据存储

应用配置、下载记录和项目日志统一保存在：

```text
<可执行文件所在目录>\application.sqlite3
```

SQLite 使用 `rusqlite` 的 `bundled` 功能构建，不要求系统预装 SQLite。

## 当前功能边界

- 设置页会真实保存 yt-dlp 路径、FFmpeg 路径、默认下载目录、代理和最大并发数。
- 检索页当前提供明确标注的演示流信息；真实格式元数据检索尚未接入。
- 创建的任务会真实写入 SQLite，并可从下载任务列表启动外部 `yt-dlp`。
- FFmpeg、代理和格式选择会作为 yt-dlp 命令参数传递。
- 最大并发数目前仅持久化，下载调度仍为单任务串行。
- 暂停操作目前不终止或伪造外部进程状态，界面会明确提示尚未支持。

## 国际化

界面文字使用 Slint 原生 bundled translations 管理，翻译资源位于：

```text
translations/<locale>/LC_MESSAGES/yt-dlp-gui.po
```

当前包含简体中文和英文。设置页可以在运行时切换语言，语言选择会保存到 SQLite，并在下次启动时恢复。新增界面文字时应使用 Slint `@tr(...)`，不要直接写不可翻译的展示字符串。

## 图标来源

界面图标来自 Microsoft 的 [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons)，采用 MIT 许可证。项目使用其单色 SVG 图标，并由 Slint 在构建时嵌入应用程序。官方图标浏览页面为 [Fluent 2 Icons](https://fluent2.microsoft.design/icons)。
