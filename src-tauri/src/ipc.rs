//! Generated JSON contracts. This initial slice covers timing preset commands;
//! the remaining command families still use the client facade's existing types.
use crate::timing_preset;

fn bindings() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::new()
        .commands(tauri_specta::collect_commands![
            timing_preset::timing_preset_open,
            timing_preset::timing_preset_save
        ])
        // Preserve the existing facade's rejected-promise contract.
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
}

#[test]
#[ignore = "explicit generator entry point; run npm run ipc:generate or ipc:check"]
fn export_bindings() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("VSF_BINDINGS_DIR").expect("generator supplies a temporary directory"),
    );
    bindings()
        .export(
            specta_typescript::Typescript::default(),
            directory.join("ipc.ts"),
        )
        .unwrap();

    // Compile actual Serde output against the generated declaration. Keep the
    // fixture readable; it checks renaming and serialization, not just derives.
    let preset: timing_preset::Preset = serde_json::from_value(serde_json::json!({
        "version": 1,
        "name": "Contract sample",
        "timing": timing_preset::Settings::default(),
    }))
    .unwrap();
    let sample = format!(
        "// Generated from Rust Serde output; do not edit.\n\
         import type {{ TimingPreset }} from '../../src/lib/client';\n\
         export const preset = {} satisfies TimingPreset;\n",
        serde_json::to_string_pretty(&preset).unwrap(),
    );
    std::fs::write(directory.join("ipc-contract-samples.ts"), sample).unwrap();
}
