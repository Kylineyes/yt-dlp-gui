mod app;

fn main() -> Result<(), slint::PlatformError> {
    app::window::run()
}
