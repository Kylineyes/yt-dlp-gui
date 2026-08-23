use crate::app::dialog::{DialogButtons, DialogRequest, DialogService, DialogTitle, DialogVisualState};
use crate::design_system::theme::{
    dark_theme_available, system_theme, TextScale as RustTextScale, ThemeMode as RustThemeMode,
};
use crate::storage::StorageError;

pub fn show_storage_error(error: StorageError) -> Result<(), Box<dyn std::error::Error>> {
    let mode = RustThemeMode::DEFAULT;
    let effective_theme = mode.resolve(system_theme(), dark_theme_available());
    let description = format!("{}\n\n请检查配置数据库路径、文件权限和 SQLite 错误详情。", error);
    let request = DialogRequest {
        title: "配置加载失败",
        description: &description,
        confirm_label: "确认",
        cancel_label: "",
        title_kind: DialogTitle::Error,
        buttons: DialogButtons::ConfirmOnly,
    };
    let _dialog = DialogService::show(
        request,
        None,
        DialogVisualState {
            effective_theme,
            text_scale: RustTextScale::Default,
        },
        |_| {
            slint::quit_event_loop().ok();
        },
    )?;
    slint::run_event_loop()?;
    Ok(())
}
