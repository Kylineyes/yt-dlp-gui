use crate::{FatalErrorWindow, MainWindow};
use slint::{ComponentHandle, Weak};

/// Owns the fatal storage window while it is visible.
pub struct FatalErrorController {
    window: Option<FatalErrorWindow>,
    main_window: Weak<MainWindow>,
    #[cfg(windows)]
    native: Option<windows_native::NativeModal>,
}

impl FatalErrorController {
    pub fn new(main_window: Weak<MainWindow>) -> Self {
        Self {
            window: None,
            main_window,
            #[cfg(windows)]
            native: None,
        }
    }

    pub fn show_or_update(&mut self, detail: String) {
        if let Some(window) = &self.window {
            window.set_fatal_detail(detail.into());
            let _ = window.show();
            #[cfg(windows)]
            if let Some(native) = &self.native {
                native.activate_and_flash();
            }
            return;
        }

        let Ok(window) = FatalErrorWindow::new() else {
            return;
        };
        window.set_fatal_detail(detail.into());
        window.set_fatal_detail_expanded(false);
        let _ = window.show();
        self.window = Some(window);
        self.ensure_native_modal();
    }

    pub fn ensure_native_modal(&mut self) {
        #[cfg(windows)]
        if self.window.is_some() && self.native.is_none() {
            self.initialize_native_modal();
        }
    }

    pub fn hide(&mut self) {
        #[cfg(windows)]
        if let Some(native) = self.native.take() {
            native.release_owner();
        }
        if let Some(window) = self.window.take() {
            let _ = window.hide();
        }
    }

    pub fn window(&self) -> Option<&FatalErrorWindow> {
        self.window.as_ref()
    }

    #[cfg(windows)]
    fn initialize_native_modal(&mut self) {
        let (Some(main), Some(fatal)) = (self.main_window.upgrade(), self.window.as_ref()) else {
            return;
        };
        self.native = windows_native::NativeModal::attach(&main, fatal);
    }
}

#[cfg(windows)]
mod windows_native {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::mem::size_of;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled};
    use windows::Win32::UI::WindowsAndMessaging::{
        DrawMenuBar, EnableMenuItem, FLASHW_ALL, FLASHW_TIMERNOFG, FLASHWINFO, FlashWindowEx,
        GWL_STYLE, GWLP_HWNDPARENT, GetSystemMenu, GetWindowLongPtrW, MF_BYCOMMAND, MF_DISABLED,
        MF_GRAYED, SC_CLOSE, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    };

    pub struct NativeModal {
        owner: HWND,
        dialog: HWND,
    }

    impl NativeModal {
        pub fn attach(main: &MainWindow, fatal: &FatalErrorWindow) -> Option<Self> {
            let owner = hwnd(main.window())?;
            let dialog = hwnd(fatal.window())?;
            unsafe {
                SetWindowLongPtrW(dialog, GWLP_HWNDPARENT, owner.0 as isize);
                let style = GetWindowLongPtrW(dialog, GWL_STYLE) as u32;
                let style = style & !WS_MINIMIZEBOX.0 & !WS_MAXIMIZEBOX.0;
                SetWindowLongPtrW(dialog, GWL_STYLE, style as isize);
                let system_menu = GetSystemMenu(dialog, false);
                if !system_menu.is_invalid() {
                    let _ = EnableMenuItem(
                        system_menu,
                        SC_CLOSE,
                        MF_BYCOMMAND | MF_DISABLED | MF_GRAYED,
                    );
                    let _ = DrawMenuBar(dialog);
                }
                let _ = SetWindowPos(
                    dialog,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                );
                // Re-assert the disabled state on every attachment and prevent a stale
                // owner state from allowing the main window to receive input.
                let _ = EnableWindow(owner, false);
                if IsWindowEnabled(owner).as_bool() {
                    return None;
                }
            }
            let modal = Self { owner, dialog };
            modal.activate_and_flash();
            Some(modal)
        }

        pub fn activate_and_flash(&self) {
            unsafe {
                let _ = SetForegroundWindow(self.dialog);
                let flash = FLASHWINFO {
                    cbSize: size_of::<FLASHWINFO>() as u32,
                    hwnd: self.dialog,
                    dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                    uCount: 3,
                    dwTimeout: 0,
                };
                let _ = FlashWindowEx(&flash);
            }
        }

        pub fn release_owner(self) {
            unsafe {
                let _ = EnableWindow(self.owner, true);
                let _ = SetForegroundWindow(self.owner);
            }
        }
    }

    fn hwnd(window: &slint::Window) -> Option<HWND> {
        let window_handle = window.window_handle();
        let handle = window_handle.window_handle().ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
            _ => None,
        }
    }
}
