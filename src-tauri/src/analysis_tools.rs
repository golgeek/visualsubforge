//! Shared development tool discovery for background media analysis.
use std::path::PathBuf;
pub fn tool(name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(format!("VSF_{}", name.to_uppercase())) {
        return path.into();
    }
    // Development fallback for GUI launches that do not inherit the shell PATH.
    // Distribution must supply tested bundled tools (parent task 003d).
    #[cfg(target_os = "macos")]
    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let path = std::path::Path::new(directory).join(name);
        if path.is_file() {
            return path;
        }
    }
    name.into()
}
