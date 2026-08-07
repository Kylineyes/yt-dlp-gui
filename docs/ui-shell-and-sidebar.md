# UI Shell and Sidebar

## Scope

This document covers `src/ui.slint` and `src/sidebar.slint`. Shared values come from [`ui-design-system.md`](ui-design-system.md).

## Shell structure

`MainWindow` is the application shell:

```text
MainWindow (1120px × 720px)
└── HorizontalBox
    ├── Sidebar (224px)
    └── active page content (remaining width)
```

The shell owns page selection, callback wiring, worker event presentation, the resolved color scheme, and the `ToastHost`. Individual pages must not duplicate the shell, choose a color scheme, or perform persistence/download work. The shell maps the persisted Follow system, Light, and Dark preference to Slint's palette; pages consume shared `Theme` tokens.

## Page routing

The `selected-page` values are stable UI contracts:

| Value | Page | Component |
|---:|---|---|
| `0` | Welcome | `WelcomePage` |
| `1` | Search | `SearchPage` |
| `2` | Download tasks | `DownloadsPage` |
| `3` | Settings | `SettingsPage` |

Do not renumber these values without updating `src/ui.slint`, `src/main.rs`, and all affected page documentation.

## Sidebar

The sidebar contains:

1. Product title: `yt-dlp Integration Platform`.
2. Product subtitle: `Download manager`.
3. Navigation buttons: Welcome, Search, Download tasks, Settings.
4. Flexible lower area.

Use the shared product-title, body, navigation, surface, spacing, and navigation-state tokens. The sidebar uses a custom `Rectangle` plus `TouchArea` rather than the default `Button` theme, so its states are explicit and stable. Each navigation item is 40px high with a 6px radius, an 8px icon/text gap, and a 3px `accent` left indicator when selected. The sidebar must not define a second button palette. The selected item uses both `surface-selected` and `accent-hover`; hover and pressed states follow the navigation table in the design system. The current monochrome icons remain unchanged; label color, background, and the left indicator provide the state distinction.

The current navigation button owns page selection. Selecting Download tasks also calls `refresh-downloads()` so the list is refreshed when entering that page. Other navigation items only change `selected-page`.

## Shell feedback

`ToastHost` is placed above page content with `z: 10`. Its lifecycle and semantic colors are defined in [`ui-feedback-and-errors.md`](ui-feedback-and-errors.md). `FatalErrorWindow` and `MessageDialog` are shell-level native-modal components managed by the main window; they must follow that document rather than introducing page-specific feedback styling.

## Acceptance criteria

- Sidebar remains visible while each page changes in the content area.
- Selected navigation is distinguishable from hover and unselected states without relying on color alone.
- Download tasks refresh when entered through the sidebar.
- Toasts remain above page content and do not change page layout.
- Both bundled languages fit the navigation controls without clipping.
