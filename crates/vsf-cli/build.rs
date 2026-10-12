fn main() {
    // Preview builds record actual linker inputs for compiler/SDK attribution.
    // This opt-in MSVC diagnostic changes neither linking choices nor runtime code.
    println!("cargo:rerun-if-env-changed=VSF_WINDOWS_LINK_EVIDENCE");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
        && let Some(directory) = std::env::var_os("VSF_WINDOWS_LINK_EVIDENCE")
    {
        let receipt = std::path::PathBuf::from(directory).join("vsf.rsp");
        println!(
            "cargo:rustc-link-arg-bin=vsf=/LINKREPROFULLPATHRSP:{}",
            receipt.display()
        );
    }
}
