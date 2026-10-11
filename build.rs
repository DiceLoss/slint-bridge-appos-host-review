fn main() {
    println!("cargo:rerun-if-env-changed=NX5_SDK_INCLUDE");
    println!("cargo:rerun-if-changed=src/backends/nx5/tde/native.c");
    #[cfg(feature = "nx5-tde")]
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("arm") {
        let include = std::env::var("NX5_SDK_INCLUDE")
            .expect("nx5-tde ARM builds require NX5_SDK_INCLUDE (matching codec headers)");
        println!("cargo:rerun-if-changed={include}");
        cc::Build::new()
            .file("src/backends/nx5/tde/native.c")
            .include(include)
            .flag("-std=gnu11")
            .warnings(true)
            .extra_warnings(true)
            .warnings_into_errors(true)
            .compile("bridge_tde");
    }
}
