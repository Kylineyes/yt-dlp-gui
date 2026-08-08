use crate::app::{
    MessageDialogAction, MessageDialogRequest, MessageDialogRequestId, MessageDialogResponse,
};
#[cfg(windows)]
use crate::native_modal::NativeModal;
use crate::{MainWindow, MessageDialog, MessageDialogButtonData, MessageDialogButtonKind};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, VecModel, Weak};
use std::collections::VecDeque;
use std::rc::Rc;

struct ActiveRequest {
    id: MessageDialogRequestId,
    buttons: Vec<MessageDialogAction>,
}

/// Owns the single general-purpose message dialog and its modal relationship.
pub(crate) struct MessageDialogController {
    window: Option<MessageDialog>,
    main_window: Weak<MainWindow>,
    active: Option<ActiveRequest>,
    queued: VecDeque<MessageDialogRequest>,
    #[cfg(windows)]
    native: Option<NativeModal>,
}

impl MessageDialogController {
    pub(crate) fn new(main_window: Weak<MainWindow>) -> Self {
        Self {
            window: None,
            main_window,
            active: None,
            queued: VecDeque::new(),
            #[cfg(windows)]
            native: None,
        }
    }

    /// Enqueues a validated request and reports whether a dialog window was created.
    pub(crate) fn enqueue(&mut self, request: MessageDialogRequest) -> bool {
        if self.active.is_some() {
            self.queued.push_back(request);
            return false;
        }

        self.active = Some(ActiveRequest {
            id: request.id,
            buttons: request.buttons.clone(),
        });
        if let Some(window) = &self.window {
            self.apply_request(window, &request);
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
            self.active = None;
            return false;
        };
        self.apply_request(&window, &request);
        window
            .window()
            .on_close_requested(|| CloseRequestResponse::KeepWindowShown);
        if window.show().is_err() {
            self.active = None;
            return false;
        }
        self.window = Some(window);
        self.ensure_native_modal();
        true
    }

    pub(crate) fn respond(&mut self, index: i32) -> Option<MessageDialogResponse> {
        let active = self.active.as_ref()?;
        let action = *active.buttons.get(usize::try_from(index).ok()?)?;
        let response = MessageDialogResponse {
            request_id: active.id,
            action,
        };

        if let Some(request) = self.queued.pop_front() {
            self.active = Some(ActiveRequest {
                id: request.id,
                buttons: request.buttons.clone(),
            });
            if let Some(window) = &self.window {
                self.apply_request(window, &request);
                let _ = window.show();
            }
        } else {
            self.hide();
        }
        Some(response)
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
        self.active = None;
        self.queued.clear();
    }

    pub(crate) fn with_window(&self, callback: impl FnOnce(&MessageDialog)) {
        if let Some(window) = self.window.as_ref() {
            callback(window);
        }
    }

    fn apply_request(&self, window: &MessageDialog, request: &MessageDialogRequest) {
        window.set_dialog_title(request.title.clone().into());
        window.set_message(request.message.clone().into());
        let buttons = request
            .buttons
            .iter()
            .copied()
            .map(|kind| MessageDialogButtonData {
                kind: button_kind(kind),
                primary: kind == MessageDialogAction::Confirm,
            })
            .collect::<Vec<_>>();
        window.set_buttons(ModelRc::from(Rc::new(VecModel::from(buttons))));
    }

    #[cfg(windows)]
    fn initialize_native_modal(&mut self) {
        let (Some(main), Some(window)) = (self.main_window.upgrade(), self.window.as_ref()) else {
            return;
        };
        self.native = NativeModal::attach(main.window(), window.window());
    }
}

fn button_kind(action: MessageDialogAction) -> MessageDialogButtonKind {
    match action {
        MessageDialogAction::Confirm => MessageDialogButtonKind::Confirm,
        MessageDialogAction::Cancel => MessageDialogButtonKind::Cancel,
        MessageDialogAction::Ignore => MessageDialogButtonKind::Ignore,
    }
}
