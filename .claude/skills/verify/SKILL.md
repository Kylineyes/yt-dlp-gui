---
name: verify
summary: Verify the Slint desktop UI through its running Windows application.
---

# Verify the desktop UI

1. Build and launch with `cargo run` or `target/debug/yt-dlp-gui.exe`.
2. On Windows, use UI Automation (`UIAutomationClient`, `UIAutomationTypes`) to enumerate text and button elements from the process main window.
3. Slint buttons may not expose `InvokePattern`; click the center of each button's automation bounding rectangle with Win32 mouse input instead.
4. Capture the app window with `System.Drawing.Graphics.CopyFromScreen` and inspect the PNG.
5. For welcome/navigation changes, verify the default page plus Welcome, Search, Download tasks, and Settings navigation in both bundled languages.

Note: `WaitForInputIdle` may throw even after the Slint window is ready. Poll `MainWindowHandle` instead of treating that exception as a launch failure.
