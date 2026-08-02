# Welcome Page

## Scope

This document covers `src/ui/welcome-page.slint`, including its internal `StepCard`. Shared visual values come from [`ui-design-system.md`](ui-design-system.md).

## Purpose

The Welcome page explains what the application does and gives a concise three-step path from configuration to download tracking. It is the default page (`selected-page == 0`).

## Structure

```text
Page container
├── page title: Welcome
├── page description
├── capability card
│   ├── section title: What this tool does
│   └── explanatory body
├── section title: How to use it
├── three StepCard components
│   ├── step number
│   ├── step title
│   └── styled description
└── footer card
    ├── technology/project acknowledgement
    └── repository link
```

Use the page-title and page-description typography tokens. Cards use the standard card surface, border, radius, padding, and section spacing. `StepCard` is a reusable page-local composition, not a new visual palette.

## Content and interaction

- Step 1 directs the user to Settings.
- Step 2 directs the user to Search and media selection.
- Step 3 directs the user to Download tasks.
- Step descriptions may use styled text for emphasized page names.
- The repository URL is an informational link and uses the shared accent/link treatment. Clicking it calls `Platform.open-url`.
- The footer acknowledges embedded libraries and external FFmpeg usage without introducing additional actions.

## Layout rules

- Keep the title and description at the top of the page container.
- Separate the capability card, steps, and footer using the standard major-section spacing.
- Keep the three steps visually equal in width and hierarchy.
- Let explanatory text wrap rather than shrinking below the body typography token.
- Avoid adding page-specific backgrounds or accent colors.

## Internationalization and acceptance

All visible copy, including step descriptions and the repository label, uses `@tr(...)`. Verify Chinese and English layouts for wrapping, card height, link visibility, and navigation references. The page must remain usable at the standard `1120px × 720px` window size.
