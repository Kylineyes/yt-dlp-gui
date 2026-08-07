# Feedback and Error Components

## Scope

This document covers `src/ui/components/feedback.slint`, `FatalErrorWindow`, and `MessageDialog` in `src/ui.slint`. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

## ValidationIndicator

`ValidationIndicator` is a 20px status icon with three states:

- `checking`: validation is in progress.
- `valid`: the field passed validation.
- `invalid`: the field requires correction.

Each state has an accessible label (`Checking`, `Valid`, or `Invalid`). The icon supplements the adjacent label and error text; it is not the only state signal.

## Toast

`ToastHost` is global shell feedback rendered above page content. It must not move or resize the page.

`ToastItem` supports:

- success: settings saved and other completed operations;
- error: operation failure;
- validation-error: settings cannot be saved until invalid fields are fixed.

Toasts use the corresponding semantic color from the design system, white readable content, shared radius and padding, and a visible close control. Success has a shorter lifetime than error and validation-error. The countdown bar and fade transitions are supporting feedback and must not be the only indication of state.

The existing lifecycle contract is:

- success lifetime is approximately 3 seconds;
- error and validation-error lifetime is approximately 6 seconds;
- manual dismiss starts the close transition;
- the host is positioned above the bottom page edge with a standard gap between toasts.

Do not create page-specific Toast implementations or colors.

## FatalErrorWindow

The fatal error window is a native-modal shell error for storage initialization failure. It contains:

1. a danger-styled title;
2. primary explanatory text;
3. a secondary recovery hint;
4. a button to show/hide the error log;
5. an OK button that closes the modal and exits the event loop;
6. an expandable read-only error log.

The error log uses a muted card surface and default border. The window may expand to show the log but must retain readable padding and button grouping. Error content must remain textual and must not rely only on red styling.

## MessageDialog

`MessageDialog` is a shell-owned, single-instance native modal for general program messages. Program code supplies a dynamic title and message; a repeated request updates and activates the existing dialog instead of stacking a second native modal.

The dialog uses `Theme.surface-window`, `Theme.text-primary` for its 20px / 600 title, and `Theme.text-secondary` for its 14px wrapping body. It uses 24px padding, 16px content spacing, and a right-aligned 36px primary `OK` button. These Theme roles must provide the same readable hierarchy in Light, Dark, and Follow system modes; do not use FatalErrorWindow's danger styling for general messages.

The `OK` button is the only dismissal path and only closes MessageDialog. The main window is disabled while the dialog is visible. The title bar's minimize, maximize, and close commands are unavailable, and Slint keeps the window shown for close requests. Confirming MessageDialog must not quit the application.

Static dialog controls use `@tr(...)`. Dynamic title and message values are runtime data; fixed application copy must still originate from translated Slint strings rather than Rust-built sentences.

## Rust/UI boundary

Rust decides when to emit worker errors, validation results, fatal storage errors, and message dialog requests. Slint presents them, starts or stops local visual timers, and emits dismiss/confirmation callbacks. Do not perform persistence or download work in these components.

## Internationalization and acceptance

All toast labels, dismiss text, fatal dialog copy, and error-log controls use `@tr(...)`. Verify success, operation failure, validation failure, manual dismiss, automatic timeout, fatal dialog expansion, and OK exit in both bundled languages. Confirm accessible status labels remain present.
