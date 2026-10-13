fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    // Prepare the fixed NX5 showcase asset on the host. Runtime decoding needs
    // only one screen-sized RGBA image; no full-size decode/resize scratch.
    let source = manifest.join("ui/assets/coastal-background.png");
    println!("cargo:rerun-if-changed={}", source.display());
    let wallpaper = output.join("coastal-background.png");
    image::open(source)
        .expect("decode showcase background")
        .resize_to_fill(1024, 600, image::imageops::FilterType::Triangle)
        .to_rgba8()
        .save(&wallpaper)
        .expect("prepare RGBA showcase background");
    std::fs::write(
        output.join("showcase-assets.slint"),
        format!(
            "export global ShowcaseAssets {{ out property <image> wallpaper: @image-url({:?}); }}",
            wallpaper.to_str().unwrap()
        ),
    )
    .expect("write showcase asset import");
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
            .with_include_paths(vec![output])
            .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles),
    )
    .expect("compile test UI");
}
