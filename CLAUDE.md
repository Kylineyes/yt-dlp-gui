# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with this repository.

## 协作语言

- 使用英文进行分析和思考。
- 使用简体中文编写 Git 提交日志、代码注释和会话回复。
- 代码中的标识符、API 名称、库名称和命令参数遵循其原有约定，不为满足中文要求而翻译。

## 项目定位与技术边界

这是一个使用 Rust 编写的 yt-dlp 图形化工具，目标运行平台为 Windows 10 版本 1903 及之后的版本。

- GUI 必须使用 Slint 方案实现。
- 禁止引入或使用任何 WebView 组件及相关技术栈。
- 新增依赖、平台 API 和构建配置必须符合 Windows 10 1903+ 的兼容性要求。
- 不要根据项目名称或 `.gitignore` 推断未被代码和配置确认的架构；以实际源代码和 Cargo 配置为准。

## 设计系统

根目录 [`design.md`](design.md) 是 UI 主题、颜色、字号、字体、行高、字间距和无障碍规则的唯一设计来源。

- 修改或新增 Slint UI、主题逻辑、设置项或控件状态前，先阅读并遵守 `design.md`。
- 新增或修改设计令牌时，必须在同一变更中同步更新 `design.md`，不得在组件中私自创建未登记的颜色、字号、字体或间距。
- 设计系统仍必须通过 Slint 落地，不得使用 WebView、HTML、CSS 或浏览器内核替代。

## 仓库当前状态

当前 checkout 仍是尚未初始化的项目骨架。跟踪树中已有 `.gitignore`、`CLAUDE.md` 和 UI 设计规范 `design.md`，尚无应用源代码、Cargo manifest、其他项目文档、测试套件、CI 配置或运行入口。

`.gitignore` 使用了 Cargo/Rust 风格的忽略规则（`target`、Rust 备份文件、MSVC `.pdb` 文件和 cargo-mutants 输出），但目前还没有 Cargo 工程。因此，添加 Rust 工程后应及时补充本文件中的实际构建、运行、格式化、检查和测试命令。

## 命令

当前没有可从项目配置中验证的构建、运行、格式化、检查、打包或测试命令。在 `Cargo.toml` 等配置文件加入前，不要假设 Cargo 或其他工具命令可以执行。

可用的仓库检查命令：

```text
git status
git diff
git log --oneline --decorate --all
git ls-tree -r --name-only HEAD
```

Cargo 工程建立后，应在这里记录经过验证的命令，包括运行单个测试文件的方法。

## 架构

当前 checkout 中尚不存在可描述的应用架构，也没有模块边界、数据流或入口点。后续架构说明应以实际 Rust/Slint 源代码和配置为依据，并明确 yt-dlp 调用、桌面界面和平台适配之间的边界；不得引入 WebView 作为界面层。

## 测试约束

测试代码必须始终以单一文件独立放置，不得合并到工程项目代码中。新增测试时，应使用项目约定的独立测试文件或测试目录，并避免在生产 Rust 源文件中加入测试模块；在项目结构确定后，补充具体的单文件测试命令。

## Git 提交规范

严格遵守 Conventional Commits 规范。提交标题使用 `<类型>[可选范围]: <简体中文描述>` 格式，例如：

```text
feat(gui): 添加下载任务列表界面
fix(download): 修复 yt-dlp 进程退出状态处理
```

类型必须使用规范类型（如 `feat`、`fix`、`refactor`、`test`、`docs`、`build`、`ci`、`chore` 或 `perf`）；提交标题和正文均使用简体中文。提交应聚焦单一变更，避免无关文件混入。

## 仓库专属要求

- 选择开发命令前先检查当前文件树和 Cargo manifest；当前仓库可能仍处于骨架阶段。
- 修改功能时保持 Windows 10 1903+ 支持，不得添加 WebView 依赖或 WebView 实现。
- 保持测试文件与工程项目代码分离，并遵守单文件测试约束。
- 项目初始化后，持续将本文件与实际命令和架构保持同步。
