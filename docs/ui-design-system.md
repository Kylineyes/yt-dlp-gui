# UI Design System

## Purpose

This document is the single source of truth for shared visual values and interaction states in the Slint UI. It applies to `src/ui.slint`, `src/sidebar.slint`, all files under `src/ui/`, and any Rust code that changes UI state or callbacks.

Page documents define information architecture and module behavior. They must reference the tokens in this document instead of introducing page-specific colors, font sizes, spacing, or control states.

## Design principles

- Keep the application calm, readable, and task-oriented.
- Use one visual language across navigation, forms, lists, cards, messages, and dialogs.
- Use blue for primary action and informational emphasis; reserve semantic colors for success, warning, and error.
- Use spacing and typography to establish hierarchy before adding decoration.
- Never communicate status by color alone; pair color with text, icon, or accessible label.
- Keep interactive states visibly distinct: default, hover, pressed, focus, selected, and disabled.

## Layout tokens

| Token | Value | Use |
|---|---:|---|
| `window-width` | `1120px` | Main application window |
| `window-height` | `720px` | Main application window |
| `sidebar-width` | `224px` | Navigation column |
| `page-padding` | `24px` | Content page outer padding |
| `space-1` | `4px` | Error text and tight inline spacing |
| `space-2` | `8px` | Control groups and icon/text gaps |
| `space-3` | `12px` | Compact content groups |
| `space-4` | `16px` | Card padding and form item spacing |
| `space-5` | `20px` | Secondary section separation |
| `space-6` | `24px` | Main section separation |
| `space-8` | `32px` | Large section separation |
| `control-height` | `36px` | Buttons, inputs, and form controls |
| `nav-control-height` | `40px` | Sidebar navigation buttons |
| `card-radius` | `8px` | Cards and selected containers |
| `control-radius` | `6px` | Buttons and input controls |
| `border-width` | `1px` | Default borders |

Use only these spacing values unless a component has a documented structural exception. Do not use arbitrary values such as `5px`, `7px`, `10px`, `13px`, or `14px` for new UI work.

## Typography tokens

The application uses the platform default UI font unless a future implementation explicitly adds a product font. New UI work must use this scale:

| Token | Size | Weight | Use |
|---|---:|---:|---|
| `font-page-title` | `28px` | `600` | Page title |
| `font-product-title` | `20px` | `600` | Sidebar product name |
| `font-section-title` | `20px` | `600` | Major section heading |
| `font-card-title` | `16px` | `600` | Card, task, or resource title |
| `font-body` | `14px` | `400` | Body copy, form text, and button text |
| `font-label` | `14px` | `600` | Form labels and navigation labels |
| `font-page-description` | `16px` | `400` | Page subtitle |
| `font-meta` | `12px` | `400` | Time, size, and tertiary metadata |

Rules:

- Do not add new `17px` or `18px` text. Use `font-card-title` unless a page document explicitly identifies a larger section heading.
- Use `font-body` for text that currently relies on a widget default.
- Use `font-meta` only for supporting information, never for required actions or error messages.
- Use `font-page-title` consistently for all four pages.
- Buttons use `font-body` with weight `500` where the widget supports it.

## Color tokens

### Theme modes

The application supports Light, Dark, and Follow system appearance preferences. Follow system delegates the active color scheme to Slint's platform integration. All custom Slint surfaces and text must consume the active `ThemeTokens` roles rather than hardcoding a light-mode color; standard widgets also follow `Palette.color-scheme`.

### Surfaces and borders

| Token | Value | Use |
|---|---|---|
| `surface-page` | `#F7F8FA` | Main page background |
| `surface-sidebar` | `#F1F3F5` | Sidebar background |
| `surface-card` | `#FFFFFF` | Cards, list containers, and inputs |
| `surface-muted` | `#F3F4F6` | Alternating rows and disabled surfaces |
| `surface-hover` | `#EFF6FF` | Hover surface for interactive controls |
| `surface-selected` | `#DBEAFE` | Selected navigation and list rows |
| `border-default` | `#D9DEE7` | Default card and control border |
| `border-hover` | `#93C5FD` | Hover border |
| `border-focus` | `#2563EB` | Focus border |

### Text

| Token | Value | Use |
|---|---|---|
| `text-primary` | `#1F2937` | Headings and primary content |
| `text-secondary` | `#5B6472` | Descriptions and secondary content |
| `text-tertiary` | `#7B8494` | Metadata and placeholders |
| `text-disabled` | `#A0A8B5` | Disabled content |
| `text-on-accent` | `#FFFFFF` | Text on accent backgrounds |

### Semantic colors

| Token | Value | Use |
|---|---|---|
| `accent` | `#2563EB` | Primary actions, links, info highlights |
| `accent-hover` | `#1D4ED8` | Hovered primary action |
| `accent-pressed` | `#1E40AF` | Pressed primary action |
| `accent-disabled` | `#BFDBFE` | Disabled primary action |
| `success` | `#15803D` | Completed and successful operations |
| `success-surface` | `#F0FDF4` | Success message background |
| `warning` | `#A16207` | Warnings and paused/non-blocking states |
| `warning-surface` | `#FEF3C7` | Warning message background |
| `danger` | `#C42B1C` | Errors, failures, and invalid fields |
| `danger-surface` | `#FEF2F2` | Error message background |
| `info-surface` | `#EFF6FF` | Informational message background |

Informational highlights always use `accent` for text/icon and `info-surface` for a local background. This includes selected media streams, current navigation, successful loading notices, and links.

## Component states

### Primary button

Use for Search, Add download task, Save settings, and the main confirmation action.

| State | Background | Text | Border |
|---|---|---|---|
| Default/clickable | `accent` | `text-on-accent` | `accent` |
| Hover | `accent-hover` | `text-on-accent` | `accent-hover` |
| Pressed | `accent-pressed` | `text-on-accent` | `accent-pressed` |
| Disabled | `accent-disabled` | `surface-hover` | `accent-disabled` |
| Focus | Current state | `text-on-accent` | `border-focus` focus ring |

### Secondary button

Use for Refresh, Browse, Load default location, Restore defaults, Start, and Pause.

| State | Background | Text | Border |
|---|---|---|---|
| Default/clickable | `surface-card` | `text-primary` | `border-default` |
| Hover | `surface-hover` | `accent-hover` | `border-hover` |
| Pressed | `surface-selected` | `accent-pressed` | `accent-hover` |
| Disabled | `surface-muted` | `text-disabled` | `border-default` |
| Focus | `surface-card` | `text-primary` | `border-focus` focus ring |

Unhighlighted buttons are neutral white controls with a gray border. They must not look like disabled controls.

### Sidebar navigation

| State | Background | Label | Selection affordance |
|---|---|---|---|
| Unselected | `transparent` | `text-secondary` | No indicator |
| Hover | `surface-hover` | `accent-hover` | No indicator |
| Selected | `surface-selected` | `accent-hover` | `accent` left indicator |
| Selected + hover | `#BFDBFE` | `accent-pressed` | `accent` left indicator |
| Disabled | `transparent` | `text-disabled` | No indicator |

Navigation uses `nav-control-height`, `control-radius`, and `space-2` icon/text spacing. Selection is shown by the selected background, label color, and a 3px `accent` left indicator. The current icon assets are monochrome, so the indicator and label provide the state distinction without relying on icon recoloring.

### Input and selection controls

| State | Background | Border | Text |
|---|---|---|---|
| Default | `surface-card` | `border-default` | `text-primary` |
| Hover | `surface-card` | `border-hover` | `text-primary` |
| Focus | `surface-card` | `border-focus` | `text-primary` |
| Disabled | `surface-muted` | `border-default` | `text-disabled` |
| Error | `surface-card` | `danger` | `text-primary` |

Placeholder text uses `text-tertiary`. Inputs and buttons share `control-height`.

### Lists and cards

- Cards use `surface-card`, `border-default`, `border-width`, and `card-radius`.
- Card padding is `space-4`.
- List rows use `surface-card` by default, `surface-muted` for optional alternating rows, `surface-hover` on hover, and `surface-selected` when selected.
- Selected rows must retain readable `text-primary` content and a visible selection affordance.
- Do not use a different accent color for a page-specific selected state.

### Messages and status

| Kind | Text/icon | Local surface |
|---|---|---|
| Information | `accent` | `info-surface` |
| Success | `success` | `success-surface` |
| Warning | `warning` | `warning-surface` |
| Error | `danger` | `danger-surface` |

Messages must include descriptive text. A status icon or accessible label supplements, rather than replaces, the text.

### Toasts

Toasts use the same semantic colors as messages, with a solid semantic background and `text-on-accent`-equivalent white content for contrast. Success toasts use `success`; error and validation-error toasts use `danger`. Toast close controls use a transparent/white treatment and remain visibly interactive on hover.

## Page framework

- `MainWindow` owns the shell and callback wiring; pages own presentation.
- The sidebar is fixed at `sidebar-width`; the active page fills the remaining width.
- Page roots use `surface-page` and `page-padding`.
- A page title is followed by its description with `space-2` separation.
- Major content sections use `space-6` separation.
- Form controls in one row use `space-2`; form fields use `space-4` separation.
- Scroll only the page content that can exceed the window height. Do not make the sidebar scroll unless the navigation model changes.

## Internationalization and accessibility

- Every user-visible string must use Slint `@tr(...)` with English source text.
- Simplified Chinese translations belong in `translations/zh-CN/LC_MESSAGES/yt-dlp-gui.po`.
- Placeholders must match between English source and every translation.
- Do not construct localized sentences in Rust.
- Provide accessible labels for status-only icons and meaningful controls.
- Do not rely on color alone for selected, valid, invalid, success, warning, or error states.
- Verify both bundled languages after UI changes.

## Color schemes

The application supports three persistent appearance preferences: `system`, `light`, and `dark`. The default is `system`, which follows the platform scheme. Choosing Light or Dark immediately previews the resolved theme; the existing Save settings action persists that preference, while Restore defaults returns it to `system`.

All shared color tokens have a light and dark value and are exposed to Slint through `src/ui/theme.slint`. Page components must consume `Theme` tokens and must not add hexadecimal colors outside that module. The deep theme uses layered navy surfaces rather than pure black: window `#121826`, page `#151C2C`, sidebar `#101725`, card `#1E293B`, default border `#3A4A63`, primary text `#F3F6FC`, secondary text `#C2CCDA`, and accent `#76AEFF`.

Text on standard surfaces must maintain at least a 4.5:1 contrast ratio. Focus borders, selection indicators, and large text must maintain at least 3:1 contrast. Success, warning, error, validation, and navigation states continue to use text, icons, or indicators in addition to color.

## Implementation and review rules

- New UI values must be added to this document before being used in a page.
- Page-specific exceptions must be documented in that page's design file with a reason.
- When implementation and this document diverge, determine whether the divergence is an intentional product change or an implementation defect; do not silently normalize one page independently.
- Shared visual changes require reviewing every page document listed in `CLAUDE.md`.
