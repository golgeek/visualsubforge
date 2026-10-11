# VisualSubForge (VSF)

A planned cross-platform subtitle editor inspired by VisualSubSync's waveform-centered workflow. Intended GitHub repository: `golgeek/visualsubforge` (not created yet).

VisualSubForge is a workshop for crafting subtitles. An initial exact-name web search on 2026-09-27 returned no results; this does not establish global name availability.

## Status

A Rust/Svelte/Tauri macOS editing workspace is implemented, with native menus, waveform analysis and SRT text/timing/history/persistence. The first video/SRT editing PoC is desktop-verified on the development Mac; see [004 evidence](docs/tasks/004-desktop-evidence.md). Audio-only short-loop timing remains a known [follow-up](docs/tasks/005-audio-loop-timing.md), and this is not a daily-use or release-ready editor. See [development setup](CONTRIBUTING.md#commands-and-toolchains) for pinned toolchains and commands, and [task 002](docs/tasks/002-development-setup.md) for validation status. Task 003 is in progress: [synthetic media and waveform analysis](docs/tasks/003a-media-analysis.md) provide the first baseline, and [003b](docs/tasks/003b-gpu-rendering.md) adds an opt-in macOS GPU playback experiment. Tauri, Svelte, libmpv, and FFmpeg remain provisional choices pending the full media feasibility milestone.

Task 006's [first increment](docs/tasks/006-first-increment-audit.md) is complete:
accepted subtitles display on video, and a separate **Follow active subtitle**
option selects/reveals the playing cue without committing drafts or stealing
focus. Editing suspends following until explicit Resume. The foundations review,
regression tests and bounded Mac/Windows/Ubuntu ARM64 desktop checks are recorded
in [006](docs/tasks/006-daily-use-workflow.md). [Cue creation/deletion/splitting/merging](docs/tasks/006-cue-operations.md) is also
implemented and validated, with reversible Rust transactions and protected drafts.
Waveform timing has bounded three-platform drag/history checks and browser cancellation regressions. Pointer click/focus fixes pass all three desktops; Windows 1 ms input, Undo/Redo and saved bytes now pass in the full editor using the native on-screen keyboard. [Recovery](docs/tasks/006-recovery.md) has background snapshots and Mac/Windows/Linux interruption/restore checks, including repeated recovery of 10,000-cue documents on all three desktops and visible live-instance handoff on all three desktops. All three desktops also pass visible storage-failure, retry and document-lifecycle checks. Mac and Ubuntu full trials and a Windows comparison with an explicitly recorded gesture deviation pass the memory and edit-acknowledgment budgets after removing unnecessary deep reactivity from accepted snapshots. All three supplied desktops have bounded visible-preview latency passes; remaining native gesture and media gates still block daily-use acceptance; see the [remaining-work tracker](docs/tasks/006-remaining-work.md).

Native media remains experimental: the Mac ANGLE/Metal path, Windows D3D11 path
and Linux GTK GLArea path all have visible preview/editing evidence using the
recorded private runtimes. Current checks include replacement, focus protection,
resize and normal shutdown. They do not establish release support, physical
Windows/Linux GPU behavior, x86-64 coverage or cross-display scaling. Linux startup now establishes libmpv’s numeric locale before GTK/workers;
locale precedence, native playback and Unicode preview are validated in the supplied VM.
The older small-desktop XWayland scale-2 command failure now has a bounded
passing comparison, as does keyboard height resizing during VFR playback.
The forced-scale main-window overflow now has a compact-layout correction and
bounded XWayland, Wayland, Mac and Windows validation. A supported Linux portal
chooser now passes bounded XWayland/Wayland open/save/cancel checks. A small
direct portal adapter distinguishes cancellation from initial service failure;
active-request service loss now has a tested correction. Loaded-media Wayland/XWayland
checks preserve drafts, focus and preview through cancellation; forced parent
destruction and input-delivery limits remain open. The Windows resize band now has a supported background-color correction
with bounded desktop validation. The legacy Mac background/selector and Mac ANGLE selector regressions also pass;
pointer delivery and other composition gates remain in
[003c](docs/tasks/003c-desktop-composition.md).

The user has approved the bounded [VSF-maintained mpv direction](docs/decisions/0007-audio-transport-clock.md#maintenance-decision-for-completing-006).
The source recipe now pins upstream `master` commit `c152964` with ten local
patches; three former backports are already upstream. The remaining audio
implementation and validation can proceed; this does not
accept unverified behavior or close the daily-use gates.

An optional three-patch audio candidate now adds CoreAudio loop-clear
continuation and EOF corrections. The Mac editor and bounded three-platform
native regressions are tested; timing/content acceptance remains open in
[005](docs/tasks/005-audio-loop-timing.md#upstream-audio-integration-checkpoint--2026-10-05).
The [Mac build guide](docs/build-macos.md) explains how to select and try it.

The pinned upstream mpv render-fence correction addresses the demonstrated
per-render leak in bounded standalone/editor checks. Other private dependency
changes, packaging, performance and adoption decisions remain open. See
[development commands](CONTRIBUTING.md), [003c](docs/tasks/003c-desktop-composition.md)
and [005](docs/tasks/005-audio-loop-timing.md) for exact tested builds and limits.
The ordinary Mac rendering experiment remains available; the current evidence
does not accept a permanent graphics or audio architecture.

Scene detection starts automatically for uncached media by default: quick image-verified
H.264 candidates, then a gentle full scan with live markers, a progress bar,
and percentage. **Tools → Scan Scene Changes** starts a fresh analysis; the same
menu offers cancellation and previous/next navigation. Completed project caches
are reused. **View → Show Scene Changes** controls purple marker visibility. [Task 008](docs/tasks/008-scene-detection.md)
records the bounded hard-cut heuristic and magnetic timing validation. Waveform
cue clicks select a range; another click inside that selected cue seeks there.
Boundary previews update enabled loops while retaining one undoable edit.

## Start here

- [Product scope](docs/product.md): workflows, first release, and deferred features.
- [Architecture](docs/architecture.md): boundaries, state ownership, and open questions.
- [Contributing](CONTRIBUTING.md): development and review workflow.
- [Build and run on macOS](docs/build-macos.md): native dependencies through application launch.
- [Task index](docs/tasks/README.md): current work and milestones.
- [Next steps](docs/tasks/next-steps.md): timing/rendering gates, daily-use editing and platform validation.
- [Legacy reference](docs/legacy-reference.md): original source revision and behavior map.
- [Decisions](docs/decisions/README.md): reasons behind important choices.
- [Agent instructions](AGENTS.md): repository working agreements.

## Repository independence

The application must build without a checkout of VisualSubSync. Its source is a reference, not a runtime or build dependency. See the legacy guide for optional checkout instructions and provenance notes.

## Licensing

The new project's license is not selected yet. The referenced VisualSubSync source carries GPL notices. Record provenance for reused code and assets and resolve the project's license before importing legacy material or publishing a release. No legacy code or assets have been imported.


Scene analysis offers **Standard detail** (default) and **Reduced detail** for the
full pass. Reduced detail can miss cuts. Both inspect every frame in the background;
see the [progressive scan evidence](docs/tasks/008-progressive-scene-analysis.md).
Raw keyframe
markers have been removed because encoder checkpoints are not scene cuts. **Save Project** stores references to video/SRT,
completed scene results, waveform options and resume position in `.vsfproject`.
**Open Project** restores paused without rescanning saved results; normal quit
updates an attached project. Save subtitles separately for interchange. See
[task 008](docs/tasks/008-scene-detection.md) for validation and current limits.


**Settings…** (⌘, on macOS; Ctrl+, on Windows/Linux) contains magnetic cue gaps,
scene buffers, portable preset import/export, automatic scanning on open and
Standard/Reduced scan detail. Find it in the macOS application menu or Edit on
Windows/Linux; Tools also links to it. Apply saves defaults across restarts; Cancel
discards the draft. Project timing settings take precedence when opening a project.
Alt/Option bypasses snapping during pointer edits. See the
[preset format](docs/timing-presets.md) and
[settings validation](docs/tasks/008-settings-and-menus.md).

File opening/saving lives in **File**; **View → Playback Diagnostics** toggles the
diagnostic panel. The desktop header retains the subtitle filename and unsaved
indicator without duplicate file-action buttons. Existing shortcuts are unchanged.
