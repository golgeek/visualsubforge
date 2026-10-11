fn main() {
    tauri_build::build();
    println!("cargo:rerun-if-changed=native/quit.mm");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .cpp(true)
            .file("native/quit.mm")
            .flag("-std=c++17")
            .flag("-fobjc-arc")
            .compile("vsf_quit_guard");
        println!("cargo:rustc-link-lib=framework=AppKit");
    }
}
