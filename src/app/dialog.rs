// 弹窗使用独立的 Slint Window，避免把原生模态行为伪装成主窗口内的覆盖层。
slint::include_modules!();

use std::cell::RefCell;
use std::rc::Rc;

use slint::winit_030::{
    winit::{raw_window_handle::HasWindowHandle, raw_window_handle::RawWindowHandle},
    WinitWindowAccessor,
};

// 标题类别只决定语义图标和状态色，不承载可见标题文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogTitle {
    Error,
    Notice,
}

// 返回值直接使用弹窗模块维护的按钮枚举，调用方无需依赖 Slint 生成类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogButton {
    Confirm,
    Cancel,
}

// 按钮布局是弹窗自身的受控能力，调用方只能选择预定义组合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogButtons {
    ConfirmOnly,
    ConfirmAndCancel,
}

// 返回静态按钮集合，保证 UI 可选按钮与业务结果枚举保持一致。
impl DialogButtons {
    pub const fn buttons(self) -> &'static [DialogButton] {
        match self {
            Self::ConfirmOnly => &[DialogButton::Confirm],
            Self::ConfirmAndCancel => &[DialogButton::Confirm, DialogButton::Cancel],
        }
    }
}

// 所有可见文案由调用方先完成 i18n 解析，弹窗只负责展示和转发交互结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogRequest<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub confirm_label: &'a str,
    pub cancel_label: &'a str,
    pub title_kind: DialogTitle,
    pub buttons: DialogButtons,
}

#[cfg(windows)]
pub type ParentWindow = windows_sys::Win32::Foundation::HWND;

#[cfg(not(windows))]
pub type ParentWindow = ();

pub struct DialogService;

impl DialogService {
    // 通过回调异步返回结果，避免在 Slint 主事件循环中嵌套阻塞式 run。
    pub fn show(
        request: DialogRequest<'_>,
        parent: Option<ParentWindow>,
        on_result: impl FnOnce(DialogButton) + 'static,
    ) -> Result<DialogWindow, slint::PlatformError> {
        let dialog = DialogWindow::new()?;
        dialog.set_dialog_title(request.title.into());
        dialog.set_dialog_description(request.description.into());
        dialog.set_confirm_label(request.confirm_label.into());
        dialog.set_cancel_label(request.cancel_label.into());
        dialog.set_title_kind(slint_title_kind(request.title_kind));
        dialog.set_button_layout(slint_button_layout(request.buttons));

        // 先注册结果和关闭回调，再显示窗口，避免用户在回调安装前完成操作。
        let callback = Rc::new(RefCell::new(Some(on_result)));
        let callback_for_result = Rc::clone(&callback);
        let dialog_weak = dialog.as_weak();
        dialog.on_result(move |kind| {
            let button = match kind {
                DialogButtonKind::Confirm => DialogButton::Confirm,
                DialogButtonKind::Cancel => DialogButton::Cancel,
            };
            restore_parent_window(parent);
            complete(&callback_for_result, button);
            if let Some(dialog) = dialog_weak.upgrade() {
                let _ = dialog.hide();
            }
        });

        dialog
            .window()
            .on_close_requested(move || slint::CloseRequestResponse::KeepWindowShown);

        dialog.show()?;
        configure_window(&dialog);
        center_window(&dialog, parent);
        set_modal_owner(&dialog, parent);
        Ok(dialog)
    }
}

// RefCell 保证系统关闭请求和按钮点击只会消费一次调用方回调。
fn complete<F>(callback: &Rc<RefCell<Option<F>>>, button: DialogButton)
where
    F: FnOnce(DialogButton) + 'static,
{
    if let Some(callback) = callback.borrow_mut().take() {
        callback(button);
    }
}

fn slint_title_kind(kind: DialogTitle) -> DialogTitleKind {
    match kind {
        DialogTitle::Error => DialogTitleKind::Error,
        DialogTitle::Notice => DialogTitleKind::Notice,
    }
}

fn slint_button_layout(layout: DialogButtons) -> DialogButtonLayout {
    match layout {
        DialogButtons::ConfirmOnly => DialogButtonLayout::ConfirmOnly,
        DialogButtons::ConfirmAndCancel => DialogButtonLayout::ConfirmAndCancel,
    }
}

fn configure_window(dialog: &DialogWindow) {
    dialog.window().with_winit_window(|window| {
        use slint::winit_030::winit::window::WindowButtons;
        window.set_enabled_buttons(WindowButtons::empty());
    });
}

fn center_window(dialog: &DialogWindow, parent: Option<ParentWindow>) {
    // 位置使用物理像素，避免 100%/150% DPI 下按逻辑像素计算产生偏移。
    let geometry = dialog.window().with_winit_window(|window| {
        let size = window.outer_size();
        let parent_rect = parent.and_then(parent_window_rect);
        let monitor_rect = window.current_monitor().map(|monitor| {
            let position = monitor.position();
            let size = monitor.size();
            (position.x, position.y, size.width as i32, size.height as i32)
        });
        (size.width as i32, size.height as i32, parent_rect, monitor_rect)
    });

    let Some((width, height, parent_rect, monitor_rect)) = geometry else {
        return;
    };
    let Some((x, y, area_width, area_height)) = parent_rect.or(monitor_rect) else {
        return;
    };

    dialog.window().set_position(slint::PhysicalPosition::new(
        x + (area_width - width) / 2,
        y + (area_height - height) / 2,
    ));
}

#[cfg(windows)]
fn parent_window_rect(parent: ParentWindow) -> Option<(i32, i32, i32, i32)> {
    let mut rect = std::mem::MaybeUninit::uninit();
    let success = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(parent, rect.as_mut_ptr()) };
    if success == 0 {
        return None;
    }
    let rect = unsafe { rect.assume_init() };
    Some((rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top))
}

#[cfg(not(windows))]
fn parent_window_rect(_parent: ParentWindow) -> Option<(i32, i32, i32, i32)> {
    None
}

#[cfg(windows)]
fn restore_parent_window(parent: Option<ParentWindow>) {
    if let Some(parent) = parent {
        unsafe {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(parent, 1);
        }
    }
}

#[cfg(not(windows))]
fn restore_parent_window(_parent: Option<ParentWindow>) {}

#[cfg(windows)]
fn set_modal_owner(dialog: &DialogWindow, parent: Option<ParentWindow>) {
    // Windows 的 owner 关系同时负责置顶、生命周期关联和模态期间禁用父窗口。
    let Some(parent) = parent else {
        return;
    };

    dialog.window().with_winit_window(|window| {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_HWNDPARENT};

        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let dialog_handle = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        unsafe {
            SetWindowLongPtrW(dialog_handle, GWLP_HWNDPARENT, parent as isize);
            EnableWindow(parent, 0);
        }
    });
}

#[cfg(not(windows))]
fn set_modal_owner(_dialog: &DialogWindow, _parent: Option<ParentWindow>) {}

#[cfg(windows)]
pub fn parent_window_handle(window: &slint::Window) -> Option<ParentWindow> {
    window.with_winit_window(|window| {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        Some(handle.hwnd.get() as ParentWindow)
    })?
}

#[cfg(not(windows))]
pub fn parent_window_handle(_window: &slint::Window) -> Option<ParentWindow> {
    None
}
