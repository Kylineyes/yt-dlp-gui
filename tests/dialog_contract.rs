use yt_dlp_gui::app::dialog::{DialogButton, DialogButtons, DialogRequest, DialogTitle};

#[test]
fn dialog_button_layout_exposes_only_supported_buttons() {
    assert_eq!(
        DialogButtons::ConfirmOnly.buttons(),
        &[DialogButton::Confirm]
    );
    assert_eq!(
        DialogButtons::ConfirmAndCancel.buttons(),
        &[DialogButton::Confirm, DialogButton::Cancel]
    );
}

#[test]
fn dialog_request_keeps_callers_localized_content_and_options() {
    let request = DialogRequest {
        title: "错误",
        description: "无法完成下载。",
        confirm_label: "确认",
        cancel_label: "取消",
        title_kind: DialogTitle::Error,
        buttons: DialogButtons::ConfirmAndCancel,
    };

    assert_eq!(request.title, "错误");
    assert_eq!(request.description, "无法完成下载。");
    assert_eq!(request.confirm_label, "确认");
    assert_eq!(request.cancel_label, "取消");
    assert_eq!(request.title_kind, DialogTitle::Error);
    assert_eq!(request.buttons, DialogButtons::ConfirmAndCancel);
}

#[test]
fn dialog_buttons_are_distinct_for_follow_up_logic() {
    assert_ne!(DialogButton::Confirm, DialogButton::Cancel);
}
