//! Opt-in observation for task 006. Records contain timings and revisions only;
//! no subtitle content or paths. Native command completion is not presentation.
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::OnceLock;

fn emit(sample: serde_json::Value) {
    // Format before writing: serde's Display emits JSON in fragments, and the
    // native renderer's C stderr writes do not share Rust's formatting lock.
    let line = format!("VSF_WORKFLOW_TRACE {sample}\n");
    // Diagnostics must not turn a closed stderr sink into an editing failure.
    let _ = std::io::stderr().lock().write_all(line.as_bytes());
}

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("VSF_WORKFLOW_TRACE").as_deref() == Ok("1"))
}

#[tauri::command]
#[specta::specta]
pub fn workflow_trace_enabled() -> bool {
    enabled()
}

#[derive(Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InputEvent {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    generation: u64,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    before_revision: u64,
    event_type: InputType,
    trusted: bool,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    event_mono_ms: f64,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    time_origin_ms: f64,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    mono_before_ms: f64,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    wall_unix_ms: f64,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    mono_after_ms: f64,
}

#[derive(Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
enum InputType {
    Click,
    Keydown,
}

impl InputEvent {
    fn valid_clock(&self) -> bool {
        [
            self.event_mono_ms,
            self.time_origin_ms,
            self.mono_before_ms,
            self.wall_unix_ms,
            self.mono_after_ms,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0)
            && self.event_mono_ms <= self.mono_before_ms
            && self.mono_before_ms <= self.mono_after_ms
    }
}

#[tauri::command]
#[specta::specta]
pub async fn workflow_trace_input(sample: InputEvent) {
    if !enabled() {
        return;
    }
    // Preserve invalid clocks as explicit diagnostic failures. This record does
    // not prove an edit was accepted: correlate beforeRevision with its ACK.
    // Formatting/stderr IO runs off both the UI and async executor threads.
    let _ = tauri::async_runtime::spawn_blocking(move || {
        emit(serde_json::json!({
            "kind": "apply-input",
            "validClock": sample.valid_clock(),
            "sample": sample,
        }));
    })
    .await;
}

#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Acknowledgment {
    operation: Operation,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    generation: u64,
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    revision: u64,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    elapsed_ms: f64,
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    started_unix_ms: f64,
}

#[derive(Deserialize, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
enum Operation {
    Open,
    Close,
    Recover,
    Save,
    Create,
    Delete,
    Split,
    Merge,
    Edit,
    Undo,
    Redo,
}

#[tauri::command]
#[specta::specta]
pub fn workflow_trace_acknowledgment(sample: Acknowledgment) {
    if enabled()
        && sample.elapsed_ms.is_finite()
        && sample.elapsed_ms >= 0.0
        && sample.started_unix_ms.is_finite()
    {
        emit(serde_json::json!({
            "kind": "document-acknowledgment",
            "operation": sample.operation,
            "generation": sample.generation,
            "revision": sample.revision,
            "elapsedMs": sample.elapsed_ms,
            "startedUnixMs": sample.started_unix_ms,
        }));
    }
}

#[cfg(all(
    feature = "media-prototype",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
pub fn preview_command(generation: u64, revision: u64, elapsed_ms: f64, success: bool) {
    if !enabled() {
        return;
    }
    let finished = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs_f64() * 1000.0);
    emit(serde_json::json!({
        "kind": "preview-command",
        "generation": generation,
        "revision": revision,
        "elapsedMs": elapsed_ms,
        "finishedUnixMs": finished,
        "success": success,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_clock_rejects_wrong_epoch_and_reversed_or_nonfinite_samples() {
        let mut sample = InputEvent {
            generation: 2,
            before_revision: 3,
            event_type: InputType::Click,
            trusted: true,
            event_mono_ms: 100.0,
            time_origin_ms: 1_790_000_000_000.0,
            mono_before_ms: 101.0,
            wall_unix_ms: 1_790_000_000_101.0,
            mono_after_ms: 101.1,
        };
        assert!(sample.valid_clock());
        sample.event_mono_ms = sample.wall_unix_ms;
        assert!(!sample.valid_clock());
        sample.event_mono_ms = 100.0;
        sample.mono_after_ms = 100.9;
        assert!(!sample.valid_clock());
        sample.mono_after_ms = 101.1;
        sample.wall_unix_ms = f64::NAN;
        assert!(!sample.valid_clock());
        sample.wall_unix_ms = 1_790_000_000_101.0;
        sample.event_mono_ms = -1.0;
        assert!(!sample.valid_clock());
    }
}
