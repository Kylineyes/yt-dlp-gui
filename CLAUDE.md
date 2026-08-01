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
- `src/storage.rs` owns SQLite schema and persistence. Application configuration, download records, and download logs live in `%LOCALAPPDATA%/yt-dlp-gui/application.sqlite3`; SQLite is built from bundled sources.
- Slint runs on the main thread. A dedicated Tokio worker thread owns SQLite and async `yt-dlp` work; background updates return through `slint::invoke_from_event_loop`.
