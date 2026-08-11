slint::include_modules!();

pub fn run() -> Result<(), slint::PlatformError> {
    AppWindow::new()?.run()
}
