# Settings Page

## Scope

This document covers `src/ui/settings-page.slint`, its `SettingLabel` composition, and settings-related UI state. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

## Purpose

The Settings page configures executable paths, the default download directory, proxy, language, and maximum concurrency. It is page `selected-page == 3`.

## Structure

```text
Scrollable page container
├── page title and description
├── required tool environment configuration field group
├── FFmpeg field group
├── default download directory field group
├── proxy field group
├── language control
├── theme control (Light / Dark / Follow system)
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

- Editing a field clears its previous save error for that field; it does not run validation immediately.
- Browse buttons only update the corresponding file or folder field.
- `invalid-revision` focuses and selects the first invalid field after a failed save.
- Save is disabled while `pending-save` is true. Rust validates all settings as one operation and persists only valid settings.
- Restore defaults updates the form and shows an informational message; it does not replace Save.
- Language selection updates the bundled translation and keeps the `language` property in sync.
- Theme selection previews Light, Dark, or Follow system immediately and persists the preference only after Save. Follow system delegates application appearance updates to Slint's platform color-scheme support.
- The concurrency control is bounded to 1–16. The current product limitation is shown as a warning message.

## Validation states

Invalid fields show translated error text using `danger` and the shared error treatment after a failed save.

The concurrency limitation uses `warning` and `warning-surface`; it is a non-blocking product notice, not a validation error.

## Internationalization and acceptance

All labels, placeholders, validation messages, warning text, and action labels use `@tr(...)`. Verify valid and invalid paths, whitespace errors, failed executable probes, directory errors, proxy errors, focus-after-save, save-time validation, restore, language switching, and long translated labels in both bundled languages.
