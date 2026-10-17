# VisualSubForge (VSF)

VisualSubForge is a subtitle editor inspired by VisualSubSync's waveform-centered
workflow. It uses Rust, TypeScript, Svelte and Tauri. This codebase is the product
foundation, with experimental native media integration; it is not release-ready.

## Current status

See the [current status and evidence table](docs/status.md) for implemented
behavior, reproducible checks, dated native results and remaining gates. That
tracked summary is the current-status authority. Historical task and decision
notes are optional local context, not prerequisites for building this checkout.

The editor provides:

- UTF-8 SRT text/timing edits, cue creation/deletion/splitting/merging and undo/redo.
  Rust owns accepted state; unapplied frontend drafts remain separate.
- Atomic subtitle saves, conflict protection between VSF instances and accepted-state
  recovery snapshots. Recovery requires Save As and does not restore drafts or
  undo history.
- Waveform navigation, timing gestures, magnetic cue/scene boundaries and optional
  active-cue following. Accepted subtitles can be previewed on native video.
- Progressive visual scene detection with Standard/Reduced detail, cancellation,
  marker navigation and completed-result caches. Reduced detail can miss cuts.
- `.vsfproject` references to media/SRT, completed scene results, view/timing
  options and paused resume position. Save subtitles separately for interchange.
  Normal quit updates an attached project and offers explicit choices on failure.
- Experimental subtitle resynchronization through a headless CLI or Tools →
  Resynchronize Subtitles, with proposal review, one-step Undo and protected Save
  As. The desktop workflow uses the loaded target video and a selected source.
- Persistent settings, portable timing and quality profiles, and native menus.
  Settings drafts are committed with Apply; projects retain their effective
  quality rules and timing options independently of workspace defaults.
- Optional accepted/draft cue metrics and a paged Problems panel with filters,
  protected cue navigation and the same checker as `vsf check`. Review works
  without media and never changes subtitles or save eligibility.
- Explicit minimum-gap previews show safe endpoint changes, skipped reasons and
  before/after metrics. Accept commits the whole plan as one Undo/Redo operation.
- Scene endpoint findings use completed scene results and show coverage limits.
  Previewable inward timing repairs preserve text/tags and apply as one Undo step.
- Duration and reading-speed previews repair a selected cue or whole document by
  moving only cue ends. Whole-document validation and explicit skips protect
  timing constraints; each accepted batch uses one Undo/Redo operation.
- Tag-aware Find/Replace, selection formatting and previewable line joining/reflow
  preserve subtitle markup. Reviewed text batches use one Undo/Redo operation.

Native Mac ANGLE/Metal, Windows D3D11 and Linux GTK playback remain opt-in
experiments. The maintained mpv recipes have pinned sources and recorded build
checks, but candidate audio/content, device, composition and distribution gates
remain open. A build, property smoke test or VM trial is not platform acceptance.

## Build, test and understand the code

- [Development workflow](CONTRIBUTING.md): pinned toolchains, default shell,
  automated checks and platform prerequisites.
- [Mac native build guide](docs/build-macos.md): SDKs, private libraries and
  explicit runtime selection.
- [Preview release preparation](docs/releases.md) and [macOS signing setup](docs/macos-signing.md):
  hosted packaging, certificate storage and GitHub Actions secrets.
- [Native maintenance contract](docs/native-runtime.md): source-build and SDK
  receipts, loaded-library identity, update ownership and adoption gates.
- [Frontend ownership walkthrough](src/lib/README.md): document/media/project
  projections, drafts, IPC and disposal.
- [Synthetic fixtures](tests/fixtures/README.md) and [native investigation index](tests/native/README.md):
  regenerable inputs, controls and diagnostic scope.
- [Timing preset format](docs/timing-presets.md): portable settings and limited
  legacy INI migration.
- [Headless resynchronization](docs/resynchronization.md): CLI commands, shared
  operation, output protections and current matching/validation limits.
- [Subtitle quality](docs/subtitle-quality.md): profiles, desktop/CLI checking,
  metric semantics and previewable minimum-gap repair.
- [Subtitle tags](docs/subtitle-tags.md): SRT preservation, inline ASS/SSA native
  preview, millisecond timing, defaults and counting limitations.
- [Text tools](docs/text-editing.md): protected search/replace, formatting drafts,
  conservative line repairs, ownership, limits and validation.
- [Scene-aware quality](docs/scene-quality.md): endpoint buffers, explicit scene
  coverage, safe timing previews and measured desktop responsiveness.
- [Duration and speed repairs](docs/duration-repairs.md): selected/all review,
  exact endpoint policy, conservative skips and validation evidence.

`subtitle-core` contains dependency-free domain/history logic; `subtitle-formats`
handles SRT parsing/serialization without filesystem access. `vsf-application`
shares bounded SRT reads, size policy and atomic file persistence with headless
callers. `src-tauri` coordinates accepted desktop state and background services,
while `media-analysis` and
`media-player` isolate FFmpeg analysis and native playback. All frontend/backend
communication goes through `src/lib/client.ts`. Decoded video frames stay out of
ordinary IPC.

File operations live in File; View offers Playback Diagnostics and scene marker
visibility. Settings is in the Mac application menu or Edit on Windows/Linux,
with a Tools shortcut. The default build can edit subtitles but has no native
playback. Browser previews use modeled IPC only in their explicit test fixtures.

## Independence and licensing

The application builds without a VisualSubSync checkout. Legacy source is an
optional behavior reference, not a build or runtime dependency, and is kept
unchanged. No legacy code or assets have been imported.

VisualSubForge's original code is licensed under the GNU General Public License,
version 3 or (at your option) any later version. See [LICENSE](LICENSE). You may
use, modify and redistribute it under those terms; it comes without any warranty.

Third-party code and dependency patches retain their own licenses, notices and
provenance. This choice does not relicense those files. A complete inventory of
the dependencies actually included in each release remains required.
