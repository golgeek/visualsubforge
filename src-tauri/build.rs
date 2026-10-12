fn main() {
    tauri_build::build();
    println!("cargo:rerun-if-env-changed=VSF_WINDOWS_LINK_EVIDENCE");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && let Some(directory) = std::env::var_os("VSF_WINDOWS_LINK_EVIDENCE")
    {
        let receipt = std::path::PathBuf::from(directory).join("visualsubforge.rsp");
        println!(
            "cargo:rustc-link-arg-bin=visualsubforge=/LINKREPROFULLPATHRSP:{}",
            receipt.display()
        );
    }
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
