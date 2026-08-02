# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Language

- Keep internal reasoning in English.
- Use Simplified Chinese for user-facing conversation and Git commit messages.
- Use English for identifiers, code comments, test names, panic/expect messages, log and error source text, and source UI translation strings.
- Add a moderate amount of English comments when writing code to explain the purpose of meaningful code blocks; avoid comments that merely restate individual lines.
- All user-visible UI copy must use Slint `@tr(...)` with English source text. Put Simplified Chinese translations in `translations/zh-CN/LC_MESSAGES/yt-dlp-gui.po`; do not construct localized sentences in Rust.
- When adding or changing UI copy, keep placeholders consistent between the English source and every PO translation.

## Project overview

This project is a graphical frontend for the `yt-dlp` library, built with Rust and Slint. The repository is currently a scaffold: no application source, dependency manifest, build configuration, or test suite has been committed yet.

## Commands

- Build/check: `cargo check`
- Run the desktop application: `cargo run`
- Format: `cargo fmt --all`
- Verify formatting: `cargo fmt --all -- --check`
- Run all tests: `cargo test --all`
- Run one test: `cargo test storage::tests::creates_download_and_logs_it`
- Lint strictly: `cargo clippy --all-targets --all-features -- -D warnings`

## Architecture

- `src/main.rs` owns the Slint event loop, translates UI callbacks into application commands, and applies worker events back to UI properties. It must not perform downloads or SQLite writes on the UI thread.
- `src/ui.slint` contains the presentation layer. Keep downloader and persistence details out of Slint.
- `src/app/` is the application/state layer. It owns command handling, download lifecycle transitions, and coordination between storage and the downloader adapter.
- `src/download/ytdlp.rs` is the sole adapter around `ytd-rs`. `ytd-rs` still launches an external `yt-dlp` executable; the application accepts an explicit executable path or resolves `yt-dlp` from `PATH`.
- `src/storage.rs` owns SQLite schema and persistence. Application configuration, download records, and download logs live in `application.sqlite3` beside the application executable; SQLite is built from bundled sources.
- Slint runs on the main thread. A dedicated Tokio worker thread owns SQLite and async `yt-dlp` work; background updates return through `slint::invoke_from_event_loop`.

## UI design documentation

All Slint layout, styling, control-state, user-visible copy, and UI callback work must read [`docs/ui-design-system.md`](docs/ui-design-system.md) first, then read the design document for the target module. The design system is the single source of truth for shared colors, typography, spacing, dimensions, surfaces, borders, and interaction states. Page documents define module structure and behavior; they must reference shared tokens instead of creating page-specific visual values.

Use this document routing:

- `src/ui.slint` or `src/sidebar.slint`: read [`docs/ui-shell-and-sidebar.md`](docs/ui-shell-and-sidebar.md).
- `src/ui/welcome-page.slint`: read [`docs/ui-welcome-page.md`](docs/ui-welcome-page.md).
- `src/ui/search-page.slint` or `StreamRow`: read [`docs/ui-search-page.md`](docs/ui-search-page.md).
- `src/ui/downloads-page.slint` or `DownloadRow`: read [`docs/ui-downloads-page.md`](docs/ui-downloads-page.md).
- `src/ui/settings-page.slint` or `SettingLabel`: read [`docs/ui-settings-page.md`](docs/ui-settings-page.md).
- `src/ui/components/feedback.slint` or `FatalErrorWindow`: read [`docs/ui-feedback-and-errors.md`](docs/ui-feedback-and-errors.md).
- `src/ui/types.slint`: read the design system and the page documents for every consumer of the changed data contract.
- Rust code that changes UI properties, callbacks, or state transitions: read the corresponding page document and, when applicable, the shell or feedback document.

For UI changes, follow this priority order: explicit user requirements, `docs/ui-design-system.md`, the target module document, then the existing implementation. If code and documentation diverge, update the relevant document first for an intentional product change; otherwise bring the implementation back into compliance. Document any page-specific exception and its reason. Before finishing, check that shared values, `@tr(...)` copy, translation placeholders, callback contracts, and affected module documents remain synchronized. Use `.claude/skills/verify/SKILL.md` for real-window UI verification when behavior or appearance changes.

## Git commit convention

Use the Conventional Commits format for all commit messages:

```text
<type>(<scope>): <imperative description>
```

- Write commit messages in Simplified Chinese, except for the required Conventional Commits type and technical identifiers.
- Use a lowercase type such as `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `build`, or `ci`.
- Add a concise scope when it clarifies the affected area, such as `ui`, `storage`, `download`, or `i18n`.
- Keep the subject concise and imperative; do not end it with punctuation.
- Use `!` for breaking changes and explain the migration in the commit body.
- Keep the commit body focused on why the change is needed and any relevant implementation details.

Examples:

```text
feat(ui): 新增默认欢迎页面
fix(storage): 修复数据库初始化失败处理
```
