# Download Tasks Page

## Scope

This document covers `src/ui/downloads-page.slint` and the `DownloadRow` data presented by it. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

## Purpose

The Download tasks page shows saved tasks, current progress, status, metadata, and task actions. It is page `selected-page == 2`.

## Structure

```text
Page container
├── header row
│   ├── page title and description
│   └── secondary Refresh button
├── page message
└── task list
    └── task card
        ├── resource name and status
        ├── URL
        ├── format
        ├── location
        ├── started/completed timestamps
        ├── progress indicator
        ├── size
        └── Start/Pause actions and error text
```

Task cards use the shared card surface, border, radius, padding, and vertical rhythm. New layout work must avoid fixed heights that clip translated or long error text; use a documented minimum height if the implementation needs one.

## Status semantics

| Status | Meaning | Token |
|---|---|---|
| `queued` | Saved and waiting to start | `text-secondary` |
| `ready` | Ready to start | `accent` |
| `running` | Currently downloading | `accent` |
| `paused` | Paused state when supported | `warning` |
| `completed` | Finished successfully | `success` |
| `failed` | Finished with an error | `danger` |

Status labels must include text. Color and progress indicators are supporting cues only.

## Interaction contract

- Refresh calls the page callback and does not alter task state locally.
- Start is enabled for `queued`, `ready`, and `failed` tasks.
- Pause is enabled only for `running` tasks.
- The page reports progress and worker log details without performing download work itself.
- A task failure displays an error message using the shared danger treatment.
- If pausing is unsupported, the page explains that it will not fake a paused state.

## Messages and metadata

Ready, count, started, log, completed, failed, unsupported pause, and task-added messages use the shared semantic message tokens. URLs, formats, and locations use body typography; timestamps and size use meta typography; the task name uses card-title typography.

## Internationalization and acceptance

All status labels, metadata prefixes, messages, and action labels use `@tr(...)`. Verify an empty list, each task status, progress, start/pause enabled states, refresh, failure text, and long translated content in both bundled languages.
