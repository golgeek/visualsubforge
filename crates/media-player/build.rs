fn main() {
    println!("cargo:rerun-if-changed=native/mpv_client.hpp");
    println!("cargo:rerun-if-changed=native/player.mm");
    println!("cargo:rerun-if-changed=native/mac_angle.hpp");
    println!("cargo:rerun-if-changed=native/mac_render_trace.hpp");
    println!("cargo:rerun-if-changed=native/mac_present_trace.hpp");
    println!("cargo:rerun-if-env-changed=VSF_ANGLE_SDK");
    println!("cargo:rerun-if-changed=native/player_windows.cpp");
    println!("cargo:rerun-if-changed=native/player_linux.cpp");
    println!("cargo:rerun-if-env-changed=VSF_MPV_SDK");
    if std::env::var_os("CARGO_FEATURE_NATIVE_MPV").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
    {
        let mut build = cc::Build::new();
        build.cpp(true).std("c++17").file("native/player_linux.cpp");
        for package in ["gtk+-3.0", "epoxy", "mpv"] {
            let library = pkg_config::Config::new()
                .probe(package)
                .unwrap_or_else(|error| {
                    panic!("Linux media-prototype requires {package}: {error}")
                });
            for path in library.include_paths {
                build.include(path);
            }
        }
        build.compile("vsf_media_surface");
    }
    if std::env::var_os("CARGO_FEATURE_NATIVE_MPV").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
    {
        let sdk = std::path::PathBuf::from(std::env::var_os("VSF_MPV_SDK").expect(
            "Windows media-prototype requires VSF_MPV_SDK with include/mpv/client.h and lib/mpv.lib",
        ));
        cc::Build::new()
            .cpp(true)
            .std("c++17")
            .flag_if_supported("/EHsc")
            .include(sdk.join("include"))
            .file("native/player_windows.cpp")
            .compile("vsf_media_surface");
        println!(
            "cargo:rustc-link-search=native={}",
            sdk.join("lib").display()
        );
        println!("cargo:rustc-link-lib=mpv");
        println!("cargo:rustc-link-lib=user32");
    }
    if std::env::var_os("CARGO_FEATURE_NATIVE_MPV").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
    {
        let library = pkg_config::Config::new()
            .atleast_version("2.0")
            .probe("mpv")
            .expect(
                "The opt-in macOS experiment requires libmpv (brew install mpv) and pkg-config",
            );
        let mut build = cc::Build::new();
        build
            .cpp(true)
            .file("native/player.mm")
            .flag("-std=c++17")
            .flag("-fobjc-arc");
        if std::env::var_os("CARGO_FEATURE_MACOS_ANGLE").is_some() {
            let sdk = std::path::PathBuf::from(std::env::var_os("VSF_ANGLE_SDK").expect(
                "macos-angle requires VSF_ANGLE_SDK with include/ and out/Metal libraries",
            ));
            build
                .define("VSF_USE_ANGLE", None)
                .include(sdk.join("include"));
            println!(
                "cargo:rustc-link-search=native={}",
                sdk.join("out/Metal").display()
            );
            println!("cargo:rustc-link-lib=EGL");
            println!("cargo:rustc-link-lib=GLESv2");
        } else {
            build.flag("-Wno-deprecated-declarations");
            println!("cargo:rustc-link-lib=framework=OpenGL");
        }
        for path in library.include_paths {
            build.include(path);
        }
        build.compile("vsf_media_surface");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
    }
}
