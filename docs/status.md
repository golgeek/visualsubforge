# Current implementation and validation status

Updated 2026-11-08 (America/New_York). VisualSubForge has an implemented subtitle
editing foundation and experimental native playback. There is no release or
daily-use acceptance yet. This matrix summarizes current scope; linked guides
retain dated commands, receipts, failures and measurements. Historical counts
are evidence for their identified source/runtime, not fresh validation of HEAD.

## Feature and platform matrix

“Mac workflow” means the bounded actual-desktop checks in the linked guide.
Browser tests use modeled IPC; automated desktop lanes supply file choices.
Real choosers and native visual observations are separate evidence where stated.
VM, software-renderer and CI checks do not establish physical-device acceptance.

| Area | Implemented scope and evidence | Remaining limits |
| --- | --- | --- |
| Subtitle formats | UTF-8 SRT, [ASS/SSA](ass-ssa-documents.md) and [WebVTT](webvtt.md), cue/history operations and same-format preservation; Mac workflow and 10k checks pass. [Tag policy](subtitle-tags.md) covers fansub SRT. | ASS/SSA uses 10 ms ticks; SRT/WebVTT use milliseconds. WebVTT CSS/region/ruby/font preview limits remain; no WebVTT conversion or settings-authoring UI. Native preview has a separate 4 MiB cap. |
| Styles, fonts and conversion | Reviewed [ASS/SSA style/font authoring](style-authoring.md), [font diagnostics](preview-fonts.md) and all six [SRT/ASS/SSA conversion directions](format-conversion.md); Mac workflow, scoped real choosers and 10k checks pass. | Unsupported attachments stay read-only; unsafe references/effects are refused. Conversion publishes a separate file after loss review. Font rendering depends on the identified runtime. |
| Text, quality and language | [Tag-aware text tools](text-editing.md), [quality profiles/Problems/gap repairs](subtitle-quality.md), [English/French tools](language-tools.md) and [statistics](statistics.md); CLI parity, Mac workflows and scale checks are recorded. | No regex/cross-cue search; ambiguous transformations stay manual. Text tools skip individual cues above 64 KiB. Statistics and quality checking do not change save eligibility. |
| Timing and scenes | [Batch timing](batch-timing.md), [scene endpoint repairs](scene-quality.md), [duration/speed repairs](duration-repairs.md) and [timing presets](timing-presets.md); accepted batches use one Undo step. Mac/10k evidence passes. | Conservative proposals are not a joint optimizer. Scene repair needs completed mapped coverage; detection is heuristic and Reduced detail can miss cuts. Timed/ambiguous effects remain protected. |
| Saves, recovery, projects and settings | One Rust accepted-state/history owner; atomic save, writer-conflict protection, durable accepted-state recovery, `.vsfproject` references and transactional settings. [Desktop checks](../tests/desktop/README.md) and [ownership](../ARCHITECTURE.md#persistence-and-recovery) describe the boundary. | Persistence is capped at 16 MiB. External writers need not honor leases. Recovery excludes drafts/history and requires Save As; no power-loss guarantee. Projects do not replace subtitle saves. |
| Legacy import and translation | Reviewed [VisualSubSync project import](legacy-project-import.md) and [manual translation references](manual-translation.md), protected copies and project/recovery integration; Mac workflows, real choosers and combined 10k checks pass. | Only the documented legacy subset; explicit missing-file relocation. Translation is manual reference matching, not machine translation. Legacy checkout remains optional and unchanged. |
| Analysis and resynchronization | Bounded FFmpeg waveform/scene analysis, verified PCM WAV origin fallback and [visual resync](resynchronization.md) for SRT/ASS/SSA via CLI/desktop. [Corpus](../tests/fixtures/resync/README.md) and [WAV checks](native-reliability.md#pcm-wav-waveform-origin--2026-10-18) record Mac/VM scope. | Visual sampling can miss indistinguishable edits; ambiguous cues stay unresolved. No audio matching. Unknown origins outside the verified PCM case remain refused. Blocking filesystem calls can delay cancellation. |
| Plugins | Bounded [checker/fix host](plugin-host.md), [modern/VSS subset](plugin-api.md), [CLI](plugin-cli.md) and [desktop review](plugin-desktop.md); Mac native/chooser/10k, ARM64 VM editor and historical-host comparison pass within scope. | Broader actions/columns/hooks/package compatibility are not implemented. Engine limits are not an OS sandbox; explicit local package selection is required. |
| Frontend and IPC | Shared feature-modal presentation, tagged document errors, one guarded cue-draft retry, generated contracts and checked numeric boundaries; [architecture](../ARCHITECTURE.md) and [frontend guide](../src/lib/README.md) describe reconciliation/disposal. | Already-dispatched Rust mutations can finish after frontend disposal. Modeled browser tests cannot prove native focus/input/composition. |
| Development Mac | Default desktop history/save/failure/recovery, guarded stale-edit retry and affected native Statistics/resync checks pass on November 8; [November regression receipt](../tests/desktop/README.md#november-refactoring-regression-checkpoint) records exact scope. | Native rerun is foreground-conditioned with generated WebView input and preview acknowledgments. Ambient readiness timeouts are not fixed; no fresh chooser, physical input, audio, latency or visual-composition acceptance. |
| Windows and Linux | Default editor/save/recovery and plugin workflows have local ARM64 VM evidence; dated hosted x64/package checks are in [release preparation](releases.md) and [native reliability](native-reliability.md). Experimental adapters have [build recipes](native-runtime.md). | VM suites were not rerun for `11f0885`; earlier evidence is baseline only. Real guest choosers, physical GPU/audio, display-scale and installation acceptance remain separate gates. |
| Native runtime and distribution | Pinned mpv recipes, SDK preflight, runtime identity, native build contracts and signed/notarized Mac preview candidates; [runtime contract](native-runtime.md), [release evidence](releases.md) and [preview acceptance](preview-acceptance.md). | Native Mac ANGLE/OpenGL, Windows D3D11 and Linux GTK remain experiments. Candidate audio/content, clocks, composition, lifetime and distribution gates remain open. A package or hosted pass is not release acceptance. |

## Remaining gates

- Resolve the retained CoreAudio loop-clear interruption and Linux Pulse clock/content
  failures on identified runtimes. The interpolation option remains unadopted;
  see [native reliability](native-reliability.md) and the
  [runtime adoption gates](native-runtime.md#remaining-adoption-gates).
- Complete held-pointer cancellation, dialog parenting/modality, lifetime,
  display-scale and composition checks. Retain the distinction between bounded
  native observations, generated input, physical interaction and presentation latency.
- Complete physical-device/GPU/audio and install/distribution acceptance for the
  claimed platforms, including source/license inventories and package identities.
  Follow [preview acceptance](preview-acceptance.md); hosted/package results alone
  do not close these gates.
- Expand visual matching/content coverage while retaining conservative refusal.
  Broader plugin compatibility and unsupported format authoring remain separate work.

The [feature roadmap](feature-roadmap.md) records completed bounded milestones and
ongoing native reliability work. The November 8 dialog/retry checkpoint preserves
Rust ownership while adding guarded cue-draft retries; the desktop guide records
its bounded validation. These checks do not accept a native runtime. Public setup
and contracts work without
local decision records, task histories, research or the local refactoring plan.
