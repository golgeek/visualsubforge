//! One command/event registry for desktop dispatch and generated JSON contracts.
use crate::{
    desktop, document, gap_review, media, preferences, project, quality_check, quality_profile,
    resync, scenes, statistics, text_review, timing_preset, timing_review, waveform,
    workflow_trace,
};

pub fn bindings() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::new()
        .commands(tauri_specta::collect_commands![
            media::media_action,
            document::document_action,
            document::recovery_status,
            document::recovery_discard,
            desktop::choose_file,
            desktop::update_menu,
            preferences::preferences_get,
            preferences::preferences_save,
            quality_profile::quality_settings,
            quality_check::quality_read,
            quality_check::quality_cancel,
            statistics::statistics_start,
            statistics::statistics_view,
            statistics::statistics_cancel,
            gap_review::gap_start,
            gap_review::gap_view,
            gap_review::gap_cancel,
            gap_review::gap_accept,
            timing_review::scene_repair_start,
            timing_review::scene_repair_view,
            timing_review::scene_repair_cancel,
            timing_review::scene_repair_accept,
            timing_review::duration_repair_start,
            timing_review::duration_repair_view,
            timing_review::duration_repair_cancel,
            timing_review::duration_repair_accept,
            text_review::text_start,
            text_review::text_view,
            text_review::text_matches,
            text_review::text_cancel,
            text_review::text_accept,
            text_review::text_format,
            quality_profile::quality_profile_open,
            quality_profile::quality_profile_save,
            desktop::finish_exit,
            timing_preset::timing_preset_open,
            timing_preset::timing_preset_save,
            project::project_open,
            project::project_attach,
            project::project_detach,
            project::project_save,
            scenes::scenes_restore,
            scenes::scenes_start,
            scenes::scenes_view,
            scenes::scenes_cancel,
            resync::resync_start,
            resync::resync_view,
            resync::resync_cancel,
            resync::resync_accept,
            waveform::waveform_start,
            waveform::waveform_view,
            workflow_trace::workflow_trace_enabled,
            workflow_trace::workflow_trace_input,
            workflow_trace::workflow_trace_acknowledgment
        ])
        .events(tauri_specta::collect_events![desktop::WorkspaceAction])
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
            specta_typescript::Typescript::default()
                .layout(specta_typescript::Layout::ModulePrefixedName),
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
    let mut sample = format!(
        "// Generated from Rust Serde output; do not edit.\n\
         import type {{ TimingPreset, DocumentAction, DocumentSnapshot, RecoveryStatus, OpenedProject, SceneView, WaveformView, WorkspaceAction, MediaStatus, Preferences, ResyncView, QualityView, GapView, SceneRepairView, DurationRepairView, StatisticsView, TextView }} from '../../src/lib/client';\n\
         export const preset = {} satisfies TimingPreset;\n",
        serde_json::to_string_pretty(&preset).unwrap(),
    );
    let mut samples = document::ipc_tests::samples();
    samples.extend([
        (
            "statisticsView",
            "StatisticsView",
            document::statistics_ipc_sample(),
        ),
        ("qualityView", "QualityView", document::quality_ipc_sample()),
        ("gapView", "GapView", document::gap_ipc_sample()),
        (
            "sceneRepairView",
            "SceneRepairView",
            document::scene_repair_ipc_sample(),
        ),
        (
            "durationRepairView",
            "DurationRepairView",
            document::duration_repair_ipc_sample(),
        ),
        ("textView", "TextView", document::text_ipc_sample()),
        ("openedProject", "OpenedProject", project::ipc_sample()),
        ("sceneView", "SceneView", scenes::ipc_sample()),
        ("waveformView", "WaveformView", waveform::ipc_sample()),
        ("resyncView", "ResyncView", resync::ipc_sample()),
        (
            "workspaceActions",
            "WorkspaceAction[]",
            desktop::ipc_sample(),
        ),
        (
            "mediaStatus",
            "MediaStatus",
            serde_json::to_value(media::Status::default()).unwrap(),
        ),
        (
            "preferences",
            "Preferences",
            serde_json::to_value(preferences::Preferences::default()).unwrap(),
        ),
    ]);
    for (name, ty, value) in samples {
        sample.push_str(&format!(
            "export const {name} = {} satisfies {ty};\n",
            serde_json::to_string_pretty(&value).unwrap(),
        ));
    }
    std::fs::write(directory.join("ipc-contract-samples.ts"), sample).unwrap();
    std::fs::write(directory.join("quality-defaults.ts"), format!(
        "// Generated from Rust profiles; do not edit.\nimport type {{ QualityProfile }} from '../client';\nexport const builtinQualityProfiles = {} satisfies QualityProfile[];\n",
        serde_json::to_string_pretty(&quality_profile::Profile::builtins()).unwrap(),
    )).unwrap();
}
