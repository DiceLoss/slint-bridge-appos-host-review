fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for name in ["startup", "alpha"] {
        for dependency in slint_build::compile_with_output_path(
            manifest.join(format!("ui/{name}.slint")),
            output.join(format!("{name}.rs")),
            slint_build::CompilerConfiguration::default(),
        )
        .expect("compile device regression UI")
        {
            println!("cargo:rerun-if-changed={}", dependency.display());
        }
    }
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::default()
            .with_style("fluent".into())
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
    )
    .expect("compile test UI");
}
