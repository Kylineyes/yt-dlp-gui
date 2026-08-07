#[cfg(windows)]
use crate::native_modal::NativeModal;
use crate::{MainWindow, MessageDialog};
use slint::{CloseRequestResponse, ComponentHandle, SharedString, Weak};

/// Owns the single general-purpose message dialog and its modal relationship.
pub(crate) struct MessageDialogController {
    window: Option<MessageDialog>,
    main_window: Weak<MainWindow>,
    #[cfg(windows)]
    native: Option<NativeModal>,
}

impl MessageDialogController {
    pub(crate) fn new(main_window: Weak<MainWindow>) -> Self {
        Self {
            window: None,
            main_window,
            #[cfg(windows)]
            native: None,
        }
    }

    /// Shows a message and reports whether a dialog window was created.
    pub(crate) fn show(
        &mut self,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
    ) -> bool {
        let title = title.into();
        let message = message.into();
        if let Some(window) = &self.window {
            window.set_dialog_title(title.clone());
            window.set_message(message.clone());
            if window.show().is_ok() {
                #[cfg(windows)]
                if let Some(native) = &self.native {
                    native.activate_and_flash();
                }
                return false;
            }
            self.hide();
        }

        let Ok(window) = MessageDialog::new() else {
            return false;
        };
        window.set_dialog_title(title);
        window.set_message(message);
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

    pub(crate) fn with_window(&self, callback: impl FnOnce(&MessageDialog)) {
        if let Some(window) = self.window.as_ref() {
            callback(window);
        }
    }

    #[cfg(windows)]
    fn initialize_native_modal(&mut self) {
        let (Some(main), Some(window)) = (self.main_window.upgrade(), self.window.as_ref()) else {
            return;
        };
        self.native = NativeModal::attach(main.window(), window.window());
    }
}
