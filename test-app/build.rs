fn main() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    for dependency in slint_build::compile_with_output_path(
        manifest.join("ui/startup.slint"),
        output.join("startup.rs"),
        slint_build::CompilerConfiguration::default(),
    )
    .expect("compile startup regression UI")
    {
        println!("cargo:rerun-if-changed={}", dependency.display());
    }
    slint_build::compile("ui/app.slint").expect("compile test UI");
}
