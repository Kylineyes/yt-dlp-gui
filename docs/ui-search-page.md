# Search Page

## Scope

This document covers `src/ui/search-page.slint` and the `StreamRow` data presented by it. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

## Purpose

The Search page accepts a video or playlist URL, displays available media streams, and creates a download task after the user chooses a stream and output directory. It is page `selected-page == 1`.

## Structure

```text
Page container
├── page title and description
├── URL input row
│   ├── URL LineEdit
│   └── primary Search button
├── resource name
├── stream list container
│   └── stream rows
├── download location label
├── output directory row
│   ├── directory LineEdit
│   ├── Load default location button
│   └── Browse button
├── primary Add download task button
└── informational or error message
```

Use standard page typography, form spacing, control height, list row, card, and message tokens. The stream list must use the shared hover and selected surfaces; do not create a second blue selection color.

## Interaction contract

- URL and Search are disabled while `search-in-progress` is true.
- Editing the URL clears the current resource, streams, selection, and message.
- A completed search supplies `resource-name` and `streams`; the user selects one row through its `TouchArea`.
- A stream row is selected when its `selected` value and `selected-stream-index` match. Selection must have a visual state and remain readable.
- Add download task is enabled only when a stream is selected and a search is not in progress.
- The output directory can be typed, loaded from the default setting, or selected through the folder dialog.
- Creating a task requires a non-empty output directory; the page reports the missing selection through the shared information/error treatment.

## Message states

The message area represents idle validation, loading, success, no formats, missing selection, missing directory, and worker failure. Informational states use `accent` and `info-surface`; failures use `danger` and `danger-surface` when a local message container exists. Text must explain the action required.

## Data display

Each `StreamRow` presents the stream id/label, video/audio availability, and estimated size. Use card-title/body/meta typography in that order. Audio-only and video-only labels remain translated source strings.

## Internationalization and acceptance

All labels, placeholders, messages, and button text use `@tr(...)`. Verify idle, loading, successful results, no-format results, row selection, missing directory, task creation, and worker failure in both bundled languages. Check that long URLs and translated stream labels wrap or truncate without hiding the controls.
