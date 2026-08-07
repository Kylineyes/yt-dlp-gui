# Feedback and Error Components

## Scope

This document covers `src/ui/components/feedback.slint` and `FatalErrorWindow` in `src/ui.slint`. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

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

## Rust/UI boundary

Rust decides when to emit worker errors, validation results, and fatal storage errors. Slint presents them, starts or stops local visual timers, and emits dismiss/confirmation callbacks. Do not perform persistence or download work in these components.

## Internationalization and acceptance

All toast labels, dismiss text, fatal dialog copy, and error-log controls use `@tr(...)`. Verify success, operation failure, validation failure, manual dismiss, automatic timeout, fatal dialog expansion, and OK exit in both bundled languages.
