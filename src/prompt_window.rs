use crate::{MainWindow, PromptWindow};
use slint::{CloseRequestResponse, ComponentHandle, SharedString, Weak};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptConfirmAction {
    Dismiss,
    Quit,
}

/// Owns the single strong-prompt window and its native owner relationship.
pub(crate) struct PromptController {
    window: Option<PromptWindow>,
    main_window: Weak<MainWindow>,
    confirm_action: PromptConfirmAction,
    #[cfg(windows)]
    native: Option<windows_native::NativeModal>,
}

impl PromptController {
    pub(crate) fn new(main_window: Weak<MainWindow>) -> Self {
        Self {
            window: None,
            main_window,
            confirm_action: PromptConfirmAction::Dismiss,
            #[cfg(windows)]
            native: None,
        }
    }

    pub(crate) fn show(
        &mut self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
    ) -> bool {
        if self.confirm_action == PromptConfirmAction::Quit && self.window.is_some() {
            #[cfg(windows)]
            if let Some(native) = &self.native {
                native.activate_and_flash();
            }
            return false;
        }
        self.show_with_action(title, message, PromptConfirmAction::Dismiss)
    }

    /// Shows the prompt and reports whether a new window was created.
    pub(crate) fn show_with_action(
        &mut self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
        action: PromptConfirmAction,
    ) -> bool {
        let title = title.into();
        let message = message.into();
        self.confirm_action = action;

        if let Some(window) = &self.window {
            window.set_prompt_title(title.clone());
            window.set_prompt_message(message.clone());
            if window.show().is_ok() {
                #[cfg(windows)]
                if let Some(native) = &self.native {
                    native.activate_and_flash();
                }
                return false;
            }
            self.hide();
            self.confirm_action = action;
        }

        let Ok(window) = PromptWindow::new() else {
            return false;
        };
        window.set_prompt_title(title);
        window.set_prompt_message(message);
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::KeepWindowShown);
        if window.show().is_err() {
            self.confirm_action = PromptConfirmAction::Dismiss;
            return false;
        }
        self.window = Some(window);
        self.ensure_native_modal();
        true
    }

    pub(crate) fn confirm(&mut self) -> PromptConfirmAction {
        let action = self.confirm_action;
        self.hide();
        action
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
        self.confirm_action = PromptConfirmAction::Dismiss;
    }

    pub(crate) fn with_window(&self, callback: impl FnOnce(&PromptWindow)) {
        if let Some(window) = self.window.as_ref() {
            callback(window);
        }
    }

    #[cfg(windows)]
    fn initialize_native_modal(&mut self) {
        let (Some(main), Some(prompt)) = (self.main_window.upgrade(), self.window.as_ref()) else {
            return;
        };
        self.native = windows_native::NativeModal::attach(&main, prompt);
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
        pub(super) fn attach(main: &MainWindow, prompt: &PromptWindow) -> Option<Self> {
            let owner = hwnd(main.window())?;
            let dialog = hwnd(prompt.window())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_confirm_actions_are_distinct() {
        assert_ne!(PromptConfirmAction::Dismiss, PromptConfirmAction::Quit);
    }
}
