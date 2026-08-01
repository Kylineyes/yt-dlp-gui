fn main() {
    println!("cargo:rerun-if-changed=src/ui.slint");
    println!("cargo:rerun-if-changed=translations");

    let config = slint_build::CompilerConfiguration::new()
        .with_default_translation_context(slint_build::DefaultTranslationContext::None)
        .with_bundled_translations("translations");
    slint_build::compile_with_config("src/ui.slint", config)
        .expect("Failed to compile the Slint UI");
}
