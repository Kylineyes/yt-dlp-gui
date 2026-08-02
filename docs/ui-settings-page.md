# Settings Page

## Scope

This document covers `src/ui/settings-page.slint`, its `SettingLabel` composition, and settings-related UI state. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

## Purpose

The Settings page configures executable paths, the default download directory, proxy, language, and maximum concurrency. It is page `selected-page == 3`.

## Structure

```text
Scrollable page container
├── page title and description
├── yt-dlp field group
├── FFmpeg field group
├── default download directory field group
├── proxy field group
├── language control
├── maximum concurrency control and warning
└── Save settings / Restore defaults actions and message
```

Each path/proxy field group follows one repeated structure:

```text
SettingLabel (validation icon + label)
input row (LineEdit and optional Browse button)
validation error text when present
```

Use the common form label, control height, spacing, error, and focus tokens. Do not create a field-specific color or spacing scale.

## Interaction contract

- Editing a field sets its validation state to checking and clears its current error kind.
- Leaving a field invokes validation for that field.
- Browse buttons invoke the corresponding file/folder callback and then trigger validation.
- `invalid-revision` focuses and selects the first invalid field after a failed save.
- Save is disabled while `pending-save` is true. Rust validates all settings and persists only valid settings.
- Restore defaults updates the form and shows an informational message; it does not replace Save.
- Language selection updates the bundled translation and keeps the `language` property in sync.
- The concurrency control is bounded to 1–16. The current product limitation is shown as a warning message.

## Validation states

`ValidationIndicator` displays checking, valid, or invalid. Invalid fields show translated error text using `danger` and the shared error treatment. The state must be understandable without color through the icon and message.

The concurrency limitation uses `warning` and `warning-surface`; it is a non-blocking product notice, not a validation error.

## Internationalization and acceptance

All labels, placeholders, validation messages, warning text, and action labels use `@tr(...)`. Verify startup validation, valid and invalid paths, whitespace errors, failed executable probes, directory errors, proxy errors, focus-after-save, save, restore, language switching, and long translated labels in both bundled languages.
