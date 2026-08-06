use crate::{FatalErrorWindow, MainWindow};
use slint::{CloseRequestResponse, ComponentHandle, SharedString, Weak};

/// Owns the fatal error window and its native owner relationship.
pub(crate) struct FatalErrorController {
    window: Option<FatalErrorWindow>,
    main_window: Weak<MainWindow>,
    #[cfg(windows)]
    native: Option<windows_native::NativeModal>,
}

impl FatalErrorController {
    pub(crate) fn new(main_window: Weak<MainWindow>) -> Self {
        Self {
            window: None,
            main_window,
            #[cfg(windows)]
            native: None,
        }
    }

    /// Shows the fatal storage error and reports whether a window was created.
    pub(crate) fn show(&mut self, error_log: impl Into<SharedString>) -> bool {
        let error_log = error_log.into();
        if let Some(window) = &self.window {
            window.set_error_log(error_log.clone());
            window.set_error_log_visible(false);
            if window.show().is_ok() {
                #[cfg(windows)]
                if let Some(native) = &self.native {
                    native.activate_and_flash();
                }
                return false;
            }
            self.hide();
        }

        let Ok(window) = FatalErrorWindow::new() else {
            return false;
        };
        window.set_error_log(error_log);
        window.set_error_log_visible(false);
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::KeepWindowShown);
        if window.show().is_err() {
            return false;
        }
        self.window = Some(window);
        self.ensure_native_modal();
        true
    }

    pub(crate) fn ensure_native_modal(&mut self) {
        #[cfg(windows)]
        if self.window.is_some() && self.native.is_none() {
            self.initialize_native_modal();
        }
    }

    pub(crate) fn hide(&mut self) {
        #[cfg(windows)]
        if let Some(native) = self.native.take() {
            native.release();
        }
        if let Some(window) = self.window.take() {
            let _ = window.hide();
        }
    }

    pub(crate) fn with_window(&self, callback: impl FnOnce(&FatalErrorWindow)) {
        if let Some(window) = self.window.as_ref() {
            callback(window);
        }
    }

    #[cfg(windows)]
    fn initialize_native_modal(&mut self) {
        let (Some(main), Some(window)) = (self.main_window.upgrade(), self.window.as_ref()) else {
            return;
        };
        self.native = windows_native::NativeModal::attach(&main, window);
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
        MF_ENABLED, MF_GRAYED, SC_CLOSE, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    };

    pub(super) struct NativeModal {
        owner: HWND,
        dialog: HWND,
        original_owner: isize,
        original_style: isize,
        owner_was_enabled: bool,
    }

    impl NativeModal {
        pub(super) fn attach(main: &MainWindow, dialog: &FatalErrorWindow) -> Option<Self> {
            let owner = hwnd(main.window())?;
            let dialog = hwnd(dialog.window())?;
            let original_owner = unsafe { GetWindowLongPtrW(dialog, GWLP_HWNDPARENT) };
            let original_style = unsafe { GetWindowLongPtrW(dialog, GWL_STYLE) };
            let owner_was_enabled = unsafe { IsWindowEnabled(owner).as_bool() };

            let mut modal = Self {
                owner,
                dialog,
                original_owner,
                original_style,
                owner_was_enabled,
            };
            if !modal.apply() {
                modal.restore_native_state();
                return None;
            }
            modal.activate_and_flash();
            Some(modal)
        }

        fn apply(&mut self) -> bool {
            unsafe {
                SetWindowLongPtrW(self.dialog, GWLP_HWNDPARENT, self.owner.0 as isize);
                let style = self.original_style as u32 & !WS_MINIMIZEBOX.0 & !WS_MAXIMIZEBOX.0;
                SetWindowLongPtrW(self.dialog, GWL_STYLE, style as isize);

                let system_menu = GetSystemMenu(self.dialog, false);
                if system_menu.is_invalid() {
                    return false;
                }
                // EnableMenuItem returns the previous state; -1 is the only failure value.
                if EnableMenuItem(
                    system_menu,
                    SC_CLOSE,
                    MF_BYCOMMAND | MF_DISABLED | MF_GRAYED,
                )
                .0 == -1
                {
                    return false;
                }
                if DrawMenuBar(self.dialog).is_err() {
                    return false;
                }
                if SetWindowPos(
                    self.dialog,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                )
                .is_err()
                {
                    return false;
                }
                if self.owner_was_enabled {
                    let _ = EnableWindow(self.owner, false);
                    if IsWindowEnabled(self.owner).as_bool() {
                        return false;
                    }
                }
            }
            true
        }

        pub(super) fn activate_and_flash(&self) {
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

        pub(super) fn release(mut self) {
            self.restore_native_state();
            if self.owner_was_enabled {
                unsafe {
                    let _ = SetForegroundWindow(self.owner);
                }
            }
        }

        fn restore_native_state(&mut self) {
            unsafe {
                let system_menu = GetSystemMenu(self.dialog, false);
                if !system_menu.is_invalid() {
                    let _ = EnableMenuItem(system_menu, SC_CLOSE, MF_BYCOMMAND | MF_ENABLED);
                    let _ = DrawMenuBar(self.dialog);
                }
                SetWindowLongPtrW(self.dialog, GWL_STYLE, self.original_style);
                SetWindowLongPtrW(self.dialog, GWLP_HWNDPARENT, self.original_owner);
                let _ = SetWindowPos(
                    self.dialog,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                );
                if self.owner_was_enabled {
                    let _ = EnableWindow(self.owner, true);
                }
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
