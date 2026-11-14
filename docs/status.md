# Current implementation status

VisualSubForge has a working subtitle-editing foundation and experimental native
playback. It is not accepted as a release-ready or daily-use editor. This page
summarizes implemented scope and remaining work, not a history of validation runs.
Exact commands, source/runtime identities, results and unavailable checks belong
in each PR description. [The test guide](../tests/desktop/README.md) explains how
to reproduce suites and what their results can establish.

Cue-list columns, widths and 24/32 px density save per layout preset. Issues and
Text stay visible; text/file-number and Issues filters preserve original cue
indices. Metrics come from Rust's checker, and ASS/SSA also expose Style/Actor.
Manual list scrolling pauses following; row clicks retain it and Jump resumes.

[Cue editing](cue-editor.md) commits on pause/ordinary blur with Rust-owned text
history grouping. Escape retains confirmed-state semantics; fixed-height issues,
source-line counts, meters and combined safe fixes remain bounded projections.
The interaction defaults still require owner trial. Nominal frame nudges do not
establish variable-rate frame-boundary accuracy.

The docked Review panel combines Issues, timing repairs, Shift and Statistics.
Live proposals stay read-only until Apply; Rust validates checked subsets and
commits one Undo step. Visible ghost/list projections are bounded to 200 cue IDs;
complete plans remain in Rust. Native video and physical-input acceptance still
require the platform-specific checks listed in each PR.

## Feature and platform matrix

| Area | Implemented scope | Remaining limits |
| --- | --- | --- |
| Subtitle formats | UTF-8 SRT, [ASS/SSA](ass-ssa-documents.md) and [WebVTT](webvtt.md), cue/history operations and same-format source preservation. [Tag policy](subtitle-tags.md) covers fansub SRT. | ASS/SSA uses 10 ms ticks; SRT/WebVTT use milliseconds. WebVTT CSS/region/ruby/font preview limits remain; no WebVTT conversion or settings-authoring UI. Native preview has a separate 4 MiB cap. |
| Styles, fonts and conversion | Reviewed [ASS/SSA style/font authoring](style-authoring.md), [font diagnostics](preview-fonts.md) and all six [SRT/ASS/SSA conversion directions](format-conversion.md). | Unsupported attachments stay read-only; unsafe references/effects are refused. Conversion publishes a separate file after loss review. Font rendering depends on the runtime. |
| Text, quality and language | [Tag-aware text tools](text-editing.md), [quality profiles/Problems/gap repairs](subtitle-quality.md), [English/French tools](language-tools.md) and [statistics](statistics.md), with desktop/CLI operations. | No regex/cross-cue search; ambiguous transformations stay manual. Text tools skip individual cues above 64 KiB. Statistics and quality checking do not change save eligibility. |
| Timing and scenes | Solid cut markers, drag-only buffers, gap lane, accepted issue stripes, full-file minimap and a video scrubber outside native bounds. [Batch timing](batch-timing.md), [scene endpoint repairs](scene-quality.md), [duration/speed repairs](duration-repairs.md) and [timing presets](timing-presets.md); accepted batches use one Undo step. | Conservative proposals are not a joint optimizer. Scene repair needs completed mapped coverage; detection is heuristic and Reduced detail can miss cuts. Timed/ambiguous effects remain protected. |
| Saves, recovery, projects and settings | One Rust accepted-state/history owner; atomic save, writer-conflict protection, durable accepted-state recovery, `.vsfproject` references and [instant Settings with Undo and shortcut presets](settings-shortcuts.md). See [persistence ownership](../ARCHITECTURE.md#persistence-and-recovery). | Persistence is capped at 16 MiB. External writers need not honor leases. Recovery excludes drafts/history and requires Save As; no power-loss guarantee. Projects do not replace subtitle saves. |
| Legacy import and translation | Reviewed [VisualSubSync project import](legacy-project-import.md), [manual translation references](manual-translation.md), protected copies and project/recovery integration. | Only the documented legacy subset; explicit missing-file relocation. Translation is manual reference matching, not machine translation. The legacy checkout remains optional and unchanged. |
| Analysis and resynchronization | Bounded FFmpeg waveform/scene analysis, verified PCM WAV origin fallback and [visual resync](resynchronization.md) for SRT/ASS/SSA through CLI/desktop. See the [synthetic corpus](../tests/fixtures/resync/README.md). | Visual sampling can miss indistinguishable edits; ambiguous cues stay unresolved. No audio matching. Unknown origins outside the verified PCM case remain refused. Blocking filesystem calls can delay cancellation. |
| Plugins | Bounded [checker/fix host](plugin-host.md), [modern/VSS subset](plugin-api.md), [CLI](plugin-cli.md) and [desktop review](plugin-desktop.md). | Broader actions/columns/hooks/package compatibility are not implemented. Engine limits are not an OS sandbox; explicit local package selection is required. |
| Frontend and IPC | Registry-backed command discovery, cue search, searchable shortcut help and delayed tooltips; shared modal/decision/menu video lifecycle, tagged document errors, draft-preserving reconciliation, generated contracts and checked numeric boundaries. [Architecture](../ARCHITECTURE.md) and the [frontend guide](../src/lib/README.md) describe ownership. | Already-dispatched Rust mutations can finish after frontend disposal. Browser models cannot prove native focus/input/composition. App creates owners and lays out regions; behavior lives with the named owners and components. |
| Default desktop platforms | Tauri editor builds and automated WebView/IPC/persistence suites for macOS, Windows and Linux; no native player is required. See [setup](../CONTRIBUTING.md) and [test scope](../tests/desktop/README.md). | A configured CI job is not evidence of execution. Supplied file choices and generated input do not establish real choosers, physical input, installation or display-scale acceptance. Startup readiness can fail before a workflow runs. |
| Native runtime and distribution | Pinned mpv recipes, SDK preflight, runtime identity, native build contracts and preview packaging. See [runtime policy](native-runtime.md), [release preparation](releases.md) and [preview acceptance](preview-acceptance.md). | Mac ANGLE/OpenGL, Windows D3D11 and Linux GTK remain experiments. Candidate audio/content, clocks, composition, lifetime and distribution gates remain open. A package or hosted pass is not release acceptance. |

## Remaining gates

- Resolve retained CoreAudio loop-clear interruption and Linux Pulse clock/content
  failures on identified runtimes. The interpolation option remains unadopted;
  see [native reliability](native-reliability.md) and the
  [runtime adoption gates](native-runtime.md#remaining-adoption-gates).
- Complete held-pointer cancellation, dialog parenting/modality, lifetime,
  display-scale and composition checks. Keep generated input, physical interaction,
  native frame observations and presentation latency distinct.
- Complete physical-device/GPU/audio and install/distribution acceptance for each
  claimed platform, including source/license inventories and package identities.
  Follow [preview acceptance](preview-acceptance.md); VM/software-renderer and
  hosted/package results alone do not close these gates.
- Expand visual matching/content coverage while retaining conservative refusal.
  Broader plugin compatibility and unsupported format authoring remain separate work.

The [feature roadmap](feature-roadmap.md) describes product scope. Public setup,
contracts and test instructions work without local task histories or research.

Workspace layout provides five user presets, per-preset pane sizes and active-preset
reset. User preferences migrate from versions 1–4 to 5 without rewriting on read.
Desktop screenshots and modeled native hide/bounds ordering are separate from
native playback/composition acceptance.

Arrange Panels supports constrained row moves/swaps, panel visibility, user defaults
and optional project layouts. It pauses and hides video until Done/Escape. Minimum
sizes are refused for new moves; saved layouts scroll when the window is smaller.
Review includes Statistics. Project saves use version 3; v1/v2 remain readable.
The custom-layout native GPU/input trials remain separate from playback-free
webview and modeled occlusion checks.
