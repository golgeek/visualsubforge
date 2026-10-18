# Current implementation and validation status

Updated 2026-10-18 (America/New_York): ASS/SSA documents, document statistics,
duration/reading-speed repairs, scene-aware
quality and timing repair,
tag-aware text editing and the first subtitle-quality milestone are complete,
including protected Find/Replace,
formatting, line repairs, inline ASS/SSA preview, quality profiles, cue metrics,
Problems review, the shared CLI and minimum-gap preview/acceptance. The first CLI/GUI resync milestone was completed
on October 12, including local three-OS checks and the Mac native workflow.
This is the tracked summary of current behavior and evidence. The code is being
evolved into the product; experimental native playback does not make persistence
or maintainability disposable. There is no release or daily-use acceptance yet.

## Implementation, checks and limits

**Next umbrella goal active (2026-10-17):** the ordered
[feature and reliability roadmap](feature-roadmap.md) covers fonts, batch timing,
ASS/SSA resync, language tools, style/attachment/conversion authoring, broader
formats/workflows, initial plugins and native reliability. Font rendering is complete within its recorded Mac runtime scope; selection and
batch timing, ASS/SSA resync, language tools and style/attachment/conversion
authoring, WebVTT and legacy project import are complete within their recorded
Mac scopes. Manual translation is in progress, followed by
plugins and native reliability.
Completed milestones below remain the
baseline. Physical Windows/Linux acceptance is a separate open release gate.

**Manual translation stage6c in progress (2026-10-18):** the application foundation
retains immutable reference bytes, format and cue data; paged time-overlap queries
support signed offsets and independent target copies. Eight focused tests and all
434 workspace Rust tests pass (30 opt-in tests ignored), with strict Clippy and
format/lint checks. Five 10k library workloads pass p95 <=86.801ms and additional
sampled RSS 99,876,864 bytes, with only 786,432 bytes of memory-budget headroom.
The subsequent Rust owner adds bounded loading/querying, stale/cancel guards,
independent target copies, sticky source protection and reference-aware recovery.
Twelve owner/related conversion cases, a signed-IPC boundary test, all 447 workspace
Rust tests /30 ignored, 44 frontend tests, strict checks, generated IPC and the
default desktop build pass. Project persistence and the panel remain pending;
the intermediate owner refuses project saves that would omit the context. No
browser/native/chooser or combined desktop10k translation acceptance is claimed.
See [the workflow contract and exact foundation evidence](manual-translation.md).

**Legacy project import stage6b complete (2026-10-18):** application parsing supports
strict UTF-8 and BOM-marked UTF-16, explicit Windows path handling, supported
references/positions and unsupported-setting diagnostics. Read-only inspection
supports explicit relocation and all four subtitle formats; revalidation detects
changed source/subtitle bytes and redirected symlinks. The new File import dialog
supports review, relocation, protected separate-file publication and explicit Open
through normal unsaved-work protection. Rust's bounded worker revalidates inputs,
protects original/competing files and preserves successful receipts after late
Cancel. Import does not modify the active document, history, policy or attachment.
425 workspace Rust tests / 30 ignored, 44 frontend tests, all 58 browser cases,
eight final focused Chromium/WebKit cases and strict/static/default desktop build
checks pass. Four earlier 10k library/IO workloads
pass <=36.236ms p95 and +50,724,864 sampled RSS bytes. Actual Mac four-format
import, relocation, source-change/failed-write protection, unchanged active state,
save/history/project/recovery and normal shutdown now pass. Separate real macOS
source/media/subtitle/output chooser checks also pass, including cancellation,
Unicode paths, exact original preservation and native restored preview. Audio-only
WAV references restore paused playback, but waveform generation reports an unknown
container start time; stage8 must address this retained limitation. Four final
desktop10k workloads pass five review/publication cycles, full project restoration,
history/save/recovery and cancel/stale checks. A retained 1656ms project-open
failure led to faster ASCII boundary scanning with every validation check retained;
final open/reopen peaks at 831ms, publication at 418ms and recovery at 422ms.
426 workspace Rust tests / 30 ignored and strict checks pass. Updated library10k
p95 is <=37.548ms with +48,267,264 sampled RSS bytes. Normal shutdown is code0.
This accepts stage6b within its recorded scope; translation remains next.
See [mapping, checks and remaining scope](legacy-project-import.md).

**WebVTT stage6a complete (2026-10-18):** open/edit/save, shared markup-aware tools,
CLI checking/statistics, project references and accepted-state recovery now retain
format identity, original blocks/settings and exact milliseconds. Text projection
decodes entities, excludes voice/ruby annotations and preserves literal ASS-looking
text. Automatic timing tools protect absolute timestamp markers; unsupported
conversion and named split/merge have explicit refusals. Preview uses the existing
decoder with visible CSS/region/ruby limitations. 398 Rust tests / 30 ignored,
44 frontend and 56 browser cases pass; final menu changes pass both browser engines.
Three expanded pure 10k workloads, including whole-document reviewed replacement
and quality evaluation, pass p95 <=100.174ms and +54,722,560 sampled RSS bytes.
Actual Mac workflow, save/reopen/project/recovery and real chooser checks pass,
including normal code0 shutdown. Independent native images show decoded CJK and
literal ASS syntax, with omitted CSS/regions, inline ruby and an explicit emoji
missing-glyph notice. Actual desktop plain/styled/timed 10k workloads pass five
edit/history/save cycles each, exact reopen/recovery and cancellation/stale guards.
All measured interactions stay below 1000ms; recovery peaks at 950ms after removing
a redundant parse. The regression and full workspace pass 399 Rust tests / 30
ignored, with strict Clippy/lint/formatting and native build checks. This closes
stage6a within the recorded Mac scope; legacy import and translation remain open.
See [scope, checks and limits](webvtt.md).

**Conversion and stage5 complete (2026-10-18):** all six SRT/ASS/SSA directions
have explicit precision/loss/blocking review and protected separate-file export.
The open source, dirty state, Undo/Redo and recovery remain unchanged. Backend
validation passes 374 Rust tests / 30 ignored; final UI checks pass 44 frontend
and 56 browser cases. Native Mac all-six workflows, failure/retry/history/project/
recovery, safe timed rounding and embedded-font observations pass. Actual macOS
SRT/ASS/SSA destination sheets, cancellation/retry, exact output and normal quit
also pass in a build without the automated chooser. A native-discovered wrapped
editor clipping defect is fixed and covered in both browser engines.

Pure 10k workloads pass p95 <=37.489ms/+81,526,784 sampled RSS bytes. Six native
10k pairs with five cycles each pass: export confirmation <=500ms, cancellation
completion <=15ms, stale invalidation <=103ms and complete 10k reopen. Native
workflow and picker runs both quit normally with code0; identified runtimes,
retained failed attempts and exact commands are recorded. No new Windows/Linux
chooser or physical install/GPU/audio acceptance is claimed.
See [conversion policy and evidence](format-conversion.md).

**Style authoring increment (2026-10-17):** reviewed
ASS/SSA create/copy/edit/rename now use Rust-owned source/cue transactions and one
Undo/Redo history. Exact references and original records are preserved; ambiguous
renames are refused. The shared text/style worker retains existing bounds and
stale/cancel guards. 344 ordinary Rust tests, 44 frontend tests, all 52 browser
cases and strict checks pass. Pure 10k create/edit/rename p95 <=25.332ms and
additional sampled RSS 77,709,312 bytes pass the predeclared gates. Actual Mac create/copy/edit/rename, exact save/project/recovery and native preview
acknowledgment pass. Native 10k acceptance <=134ms and Undo <=100ms pass the
unchanged interaction gate. Subsequent attachment and conversion acceptance
completes stage5. See [style policy and evidence](style-authoring.md).

**Font attachment increment (2026-10-18):**
reviewed ASS/SSA inventory/add/replace/remove shares the existing bounded review
worker and source/history owner. Supported single TTF/CFF/CFF2 fonts have explicit
limits; unsupported records remain read-only. 358 ordinary Rust, 44 frontend and
54 browser cases plus strict checks pass. Actual Mac both-format save/history/
project/recovery and same-player replacement/removal pass; independent native
window observations show changed/restored shapes. Embedded-only software checks
include CFF2. Pure 10k p95 <=11.569ms/+72,433,664 sampled RSS bytes; native 10k
acceptance <=129ms, Undo <=101ms and stale invalidation <=107ms pass unchanged
gates. Final normal quit is code 0. The first harness quit failure and its corrected
UI mutation are retained. See [scope, exact commands and evidence](style-authoring.md).

**Language tools complete (2026-10-17):** opt-in
English/French checking, source-mapped Problems, bounded selected suggestions,
reviewed one-occurrence corrections and persistent/session lexicons now share the
existing Rust workers, context guards and Undo path. Browser and library tests
cover Unicode/markup, failed persistence, stale reviews and exact SRT/ASS/SSA
preservation. The six-format/language 10k workload passes 500ms/96MiB gates after
fixing findings-buffer retention. Actual Mac six-case language workflows include
all optional rules, save/reopen, project/recovery, failed writes and lexicon
persistence. Six native 10k cases pass existing interaction gates: acceptance
<=122ms, cancellation <=16ms and stale invalidation <=69ms. Final library p95
analysis/proposal <=76.695/158.380ms with 48,037,888 additional RSS bytes.
[Scope, exact commands and retained failures](language-tools.md) close stage4;
Windows/Linux new-feature and physical acceptance remain separate.

**ASS/SSA resync complete (2026-10-17):** shared CLI/GUI analysis preserves source
styles, metadata, attachments and tags. Rational endpoints round directly to the
stated source grid. Cues with timed/ambiguous effects can move intact; duration
changes require manual review and block whole-document acceptance, including
ASS tags inside SRT. Accepted batches retain source protection and one-step Undo.

Stage3 validation: 313 ordinary Rust tests, 44 frontend tests, 50 browser cases,
real FFmpeg/CLI matrices and strict static/build/IPC checks pass. Actual Mac
ASS/SSA/tagged-SRT review, acceptance, exact CLI/save-byte parity, failed-save
preservation, Undo/Redo, preview acknowledgment and timed-effect refusal pass.
10k proposal p95 is <=103.088ms with 32,931,840 additional sampled RSS bytes,
within 500ms/96MiB budgets. See [policy and exact evidence](resynchronization.md#assssa-extension-validation--2026-10-17).
Windows/Linux new-feature execution and physical installation/GPU/audio acceptance
remain separate open gates; the approved native runtime strategy is unchanged.

**Selection and batch timing complete (2026-10-17):** mouse/keyboard ranges and
disjoint selection retain surviving cue IDs through edits/history. Explicit
selected/all/following-row/from-playhead shifts and selection alignment show exact
before/after timings. Rust rejects invalid batches atomically and accepts each as
one Undo step, retaining text/tags/styles/metadata/attachments and format precision.
The existing bounded timing worker handles cancellation and stale contexts.

308 ordinary Rust tests, 44 frontend tests, all 50 browser cases and strict
static/build/IPC checks pass. Actual Mac disjoint alignment, save/error/history and
10k ASS/SSA workflows pass: preview <=317ms, accept <=161ms, Undo <=106ms; release
p95 <=46.586ms and additional sampled RSS 54,018,048 bytes meet unchanged budgets.
See [batch timing and exact evidence](batch-timing.md). Windows/Linux execution,
physical input/presentation and distribution acceptance remain separate gates.

**Font milestone complete (2026-10-17):** the current Mac runtime's generic CJK
fallback now uses an installed last-resort font without rewriting original font
names or redistributing fonts. Bounded session notices expose unavailable families,
substitutions and missing glyphs. Actual Mac generic/explicit/embedded rendering,
attachment replacement, exact save/Undo, notice reset and normal resource cleanup
pass. Abrupt termination can leave the private temporary font link for OS cleanup.
301 ordinary Rust tests, 41 frontend tests, 48 browser cases, strict static/IPC
checks and the unchanged 10k desktop interaction gate pass. See [font policy and
exact evidence](preview-fonts.md). Windows/Linux rendering, ANGLE and distribution
acceptance remain separate; later historical results below describe their original
runtime state.

**ASS/SSA document milestone complete (2026-10-17):** loss-preserving UTF-8
ASS v4.00+/SSA v4.00 parsing and same-format serialization retain original script
headers, styles, event fields, comments, attachments and unknown content. Rust owns
metadata/history; style inspection/assignment and safe text/timing tools share
existing owners. Save/Save As, projects and recovery retain format identity.
Fansub SRT with inline ASS/SSA tags retains exact text and millisecond endpoints.

Final validation passes 300 ordinary Rust tests, 41 frontend tests, 48 Chromium/
WebKit cases, strict Clippy/lint/format/type/IPC checks and the default desktop
build. Actual Mac ASS/SSA inspection/edit/Undo/Redo/save/reopen/project/crash-recovery
workflows and tagged-SRT native/save/history regression pass. Native window
observations show source-defined style/alignment, position, hard breaks, motion
and expiry. Generic sans-serif CJK fallback still produces boxes on this runtime;
an explicit installed Hiragino Sans GB control renders the glyphs correctly.

The exact 10,000-cue desktop gate passes: open 260/285 ms (ASS/SSA), text/style/time
acceptance <=105 ms, Undo <=96 ms, navigation <=27 ms, cancellation <=15 ms and
invalidation <=59 ms. Rebuilt release format p95 is 29.723/25.165 ms with 46,333,952
additional sampled RSS bytes, within unchanged 500 ms/96 MiB budgets. Measurements
and retained failures are in the [ASS/SSA integration guide](ass-ssa-documents.md).
Style design, attachment management and conversion were deferred at that milestone;
their subsequent increments and ASS/SSA resync are recorded above. Windows/Linux feature execution, Linux picker
filters, installation and physical GPU/audio acceptance remain unverified.

**Statistics milestone complete (2026-10-17):** read-only whole-document or
inclusive cue-range reports share Rust quality metrics and the active profile
with `vsf stats`. Counts/denominators, exclusions and unavailable rates are
explicit. Duration/CPS/RS and line/gap summaries, nine RS categories, all-pair
overlaps and paged outliers preserve accepted state, tags and drafts. Cue links
use existing draft protection. One bounded worker rejects stale document/profile/
media/scene results without interrupting checker or repair jobs.

Validation passes 271 ordinary Rust tests, 39 frontend unit tests, 46 browser
cases, strict Clippy/lint/type/format/IPC checks and the default desktop build.
The actual Mac workflow verifies three-scope CLI parity, draft guards, source/
history preservation, profile/media/rescan invalidation and recovery regression.
At 10,000 cues/cuts, display takes 449 ms, draft interaction 4 ms, cancellation
1 ms, invalidation 12 ms and navigation 125 ms. Release document/range p95 is
11.003/9.675 ms with 21,676,032 additional sampled RSS bytes. The initial RSS
failure is retained; bounded allocation fixes it without changing the budgets.
See [statistics](statistics.md) for policy, commands, ownership and limits.
New Windows/Linux feature execution, clean installs and physical GPU/audio
acceptance remain open.

**Duration/speed milestone complete (2026-10-17):** selected-cue and whole-document
previews keep starts fixed and choose the nearest integer-ms end satisfying the
active duration/CPS/RS limits. Shared whole-document checking protects neighboring
cues, overlap, gap and scene constraints. Unavailable scenes, ambiguous overlaps,
timing-sensitive markup and impossible/coupled plans have explicit skips. Static
text/tags, cue identities/order and save/recovery protections are preserved; each
accepted batch is one Undo/Redo. Scene and duration review share one bounded
worker with purpose guards, cancellation and document/profile/media/scene checks.

Validation passes 255 Rust tests, 39 frontend unit tests, 44 Chromium/WebKit cases,
strict Clippy/lint/type/format/IPC checks and the default desktop build. The actual
Mac workflow passes selected/all review, exact save/reopen, failed saves, history,
scene/media invalidation and the existing scene/text/recovery regression. With
10,000 cues/cuts, duration preview takes 418 ms, acceptance 113 ms, draft interaction
1 ms, cancellation 3 ms and invalidation 13 ms. Release extension/trim p95 is
23.209/20.371 ms with 40,255,488 additional sampled RSS bytes. The initial RSS
failure and allocator investigation remain recorded; the unchanged budgets pass.
See [duration repairs](duration-repairs.md) for policy, ownership and exact evidence.
Windows/Linux feature execution, installations and physical GPU/audio acceptance
remain unverified; full ASS/SSA documents/styles remain separate.

**Scene-quality milestone complete (2026-10-17):** shared Rust endpoint checks
use active profile buffers and current completed scenes. Exact outer buffer
boundaries are allowed; crossing a cut alone is valid. Problems distinguishes
missing, partial and heuristic coverage. Immutable previews show inward repairs,
before/after metrics and explicit skips. Whole-document validation protects
duration, speed, overlap, gap and scene constraints; text/tags, cue identities,
order and save/recovery protections are preserved. Each accepted batch is one
Undo/Redo. Media/scene changes invalidate checks and proposals independently of
document revisions; bounded workers reject stale/late work.

Validation passes 236 Rust tests, 39 frontend unit tests, 42 Chromium/WebKit cases,
strict Clippy/lint/type/format/IPC checks and the default desktop build. The actual
media-enabled Mac workflow verifies Problems, static/timed tag handling,
preview/acceptance, failed saves, exact save/reopen, Undo/Redo, rescan/media
invalidation and the existing restart/recovery workflow. With 10,000 cues/cuts,
preview takes 413 ms, acceptance 110 ms, draft interaction 1 ms, cancellation
3 ms and invalidation 13 ms. Release check/repair p95 is 7.124/22.531 ms, with
37,257,216 additional sampled RSS bytes; declared budgets pass. Quality report
schema 2 adds scene coverage: the CLI returns incomplete coverage for nonzero
scene buffers because it supplies no scene data. See [scene quality](scene-quality.md)
for exact commands, retained evidence and the Mac OpenGL runtime scope. New
Windows/Linux feature execution and physical GPU/audio acceptance are unverified.

**Text editing milestone complete (2026-10-17):** literal Unicode Find/Replace
maps matches back to original source; supported tags/escapes stay protected and
ambiguous replacements explain why they are skipped. Single-match and Replace
All acceptance use one atomic Rust history operation. Formatting shortcuts update
selected draft text; explicit Apply accepts it. Conservative line joining/reflow
uses the active profile, previews exact changes and leaves positioned text,
dialogue, karaoke and other ambiguous cases for manual review. Document/profile
guards, bounded cancellation, failed-save preservation and quality invalidation
are tested. Full ASS/SSA files/styles remain separate.

Final validation passes 218 Rust tests, 39 frontend unit tests, all 40 browser
workflows, strict Clippy/lint/type/format/IPC checks and native-media compilation.
The actual Mac application passes protected navigation, exact tag preservation,
single/all acceptance, line repairs, formatting, Undo/Redo, failed saves, restart,
recovery and normal shutdown. With 10,000 cues, text preview takes 849 ms,
acceptance 114 ms, cancellation 4 ms and edit invalidation 14 ms. Release proposal
p95 is 16.600–38.144 ms across four workloads, with 30,883,840 additional sampled
RSS bytes; declared CPU/RSS/interaction budgets pass. See the
[text tool contract and receipts](text-editing.md). New Windows/Linux text UI
execution, installations and physical GPU/audio acceptance remain unverified.

**Quality milestone complete (2026-10-17):** shared pure Rust metrics/rules,
strict version-2 data-only profiles, default/partial archived SW profiles, and
`vsf check` now have tests. Desktop Settings supports profile selection,
import/export, customization and transactional Apply; Rust owns the accepted
policy and project snapshots. Accepted/labeled draft metrics and Problems now
support filters, paged findings and protected distinct-cue navigation, including
without media. One cancellable worker rejects obsolete document/profile results;
checking leaves history and save policy unchanged. Explicit immutable gap proposals
now have protected desktop review, bounded pages, context guards and one-step
Undo/Redo acceptance. Safe changes and skipped reasons are shown before acceptance.
Inline ASS/SSA tag preservation
and native preview are implemented, with exact millisecond demux and software
composition checks; full ASS/SSA files/styles remain separate. See the
[tag contract and validation limits](subtitle-tags.md). The 10,000-cue streaming CPU/report/RSS benchmark passes its declared
budgets; full-buffer benchmark memory failed and remains recorded. See the
[quality contract and commands](subtitle-quality.md). Native-platform release
acceptance remains separate.

At its closeout, the quality milestone passed 203 Rust tests, 39 frontend unit tests and all 38
Chromium/WebKit workflows, strict Clippy/lint/type/IPC checks and a real Mac
Settings/metrics/Problems/gap-acceptance/save/restart/recovery workflow. That run compares every
desktop metric/finding and profile hash with the actual CLI on the same input.
Its native media configuration compiles. The 10,000-cue debug desktop run measures
101 ms p95 check IPC round trips, 16 ms draft interaction, 181 ms cancellation and
56 ms edit/result invalidation; the interaction checks pass the declared one-second
budget. It also reviews 9,999 gap candidates in bounded pages. A reproduced compact
window overlap with metrics/recovery is fixed; final browser and desktop checks
keep history controls reachable. New Windows/Linux quality UI execution and physical
GPU/audio checks remain unverified. Full ASS/SSA file import remains out of scope.

**Mac preview regression fixed (2026-10-12):** the original signed candidates
from 38205417832 are withdrawn. A synthetic video reproduced `CODESIGNING /
Invalid Page` in mpv's unused LuaJIT `stats` script without a project. Shared
native initialization now disables these scripts without JIT entitlements.
Both replacement architectures pass hardened initialization, notarization and
independent signature/source audits. The exact signed ARM download passes the
native smoke and restores the reported version-1 project/version-3 scene cache
on macOS 27, showing video, waveform and five cues at the saved position. Normal
quit succeeds; original project/media/subtitle hashes remain unchanged. No
migration is needed. [Replacement downloads and remaining manual gates](preview-acceptance.md).

| Area | Implemented foundation and reproducible checks | Remaining limit or gate |
| --- | --- | --- |
| Subtitle editing | Rust owns accepted UTF-8 SRT/ASS/SSA/WebVTT, cue metadata/identity, generation/revision and undo/redo. Reviewed ASS/SSA style/font authoring shares history. Actual Mac format/save/history/recovery and 10k workflows pass. Separate-file SRT/ASS/SSA conversion also passes native and real chooser checks. | UTF-8 only; ASS/SSA endpoints require 10 ms precision. Unsafe operations are refused. WebVTT settings/blocks are preserved without an authoring UI or conversion; preview has documented CSS/region/ruby/font limits. Native preview has a separate 4 MiB limit. |
| Duration/speed repair | Selected/all fixed-start previews, exact shared metric bounds, whole-document validation, explicit skips and atomic Undo/Redo. Actual Mac and 10,000-cue workflows pass. | Conservative plan, not a joint optimizer or dialogue synchronization. Completed scenes required with buffers enabled; timed/unknown markup stays manual. |
| Scene quality | Shared endpoint checks, explicit coverage and paged inward-repair previews use current completed scenes. Safe whole batches preserve text/tags and use one Undo/Redo. Real Mac and 10,000-cue workflows pass. | Heuristic detection can miss cuts. Missing/partial data cannot authorize repair; timed/ambiguous markup stays manual. No new Windows/Linux execution. |
| Text tools | Protected literal Find/Replace, Unicode source navigation, formatting drafts and previewable join/reflow share Rust transformations. Whole batches preserve timing/identity/metadata and use one Undo/Redo operation. SRT real Mac and 10,000-cue workflows pass; format-aware ASS/SSA tools have shared/owner/browser coverage. | No regex or cross-cue search; ambiguous transformations are skipped. Text tools skip individual cues above 64 KiB; global persistence remains 16 MiB. ASS/SSA desktop and 10,000-cue validation pass within the format policy above. |
| Saves and recovery | Acceptance checks the shared 16 MiB persistence limit. Atomic saves compare prior bytes under a stable sibling lease. Conflicting VSF writers preserve the losing edit. Background recovery uses instance leases and stale-checkpoint guards. Local Mac/Ubuntu/Windows ARM64 suites and actual default-app save/recovery workflows pass. | External editors need not honor advisory locks. Recovery excludes unapplied drafts, undo history and edits since the last durable checkpoint; no power-loss guarantee. |
| Projects and settings | Rust owns project attachment tokens and byte baselines; incomplete restore cannot attach. Quit offers retry/copy/cancel/explicit exit after project-save failure. Settings apply only after successful validated persistence. | Project metadata is separate from subtitle documents/recovery. Settings retain last-successful-write behavior across instances. New quit-failure choices have modeled browser coverage; real OS close/modal coverage remains. |
| Frontend ownership | Scoped document, media and project owners retain projection/draft/response ordering and explicit disposal. One typed IPC client, pure timeline calculations and nine component/lifecycle pages have automated checks. | Modeled IPC/synthetic events do not establish native input, focus, dialogs or presentation. Already-dispatched Rust work can finish after frontend disposal. |
| Analysis | Waveform and visual scene analysis use cancellable FFmpeg processes, generation guards, one worker plus one pending request per service, and coalescing. Four waveform and seven scene integrations run actual FFmpeg. | Cancellation bounds admitted jobs, not the latency of blocking filesystem IO. Scene cuts are heuristic; Reduced detail can miss cuts. |
| Resynchronization | Independent edit-list fixtures, bounded scene-signature matching, sampled verification and a headless evaluator in `media-analysis`; [commands and criteria](../tests/fixtures/resync/README.md). The v4 verifier passes 29 synthetic cases on Mac/Ubuntu/Windows ARM64 and both reference films on Mac. | Coverage is measured, not universal. Sampled visual evidence can miss indistinguishable edits; ambiguous or unsupported cues remain unresolved. Wider real-programme evaluation is a follow-up. |
| Shared application and CLI | `vsf-application` owns shared format/persistence policies, immutable resync proposals, bounded/cancellable FFmpeg orchestration, content identities, reports and no-replace output publication. `vsf resync` calls it directly without desktop/native playback dependencies. All cue endpoints and within-section gaps follow measured content timing; nominal FPS alone never determines speed. Hosted headless checks pass on macOS ARM64 and Windows/Ubuntu x64 at `a222eda`. Shared quality checking and conservative gap proposals also power the desktop. | Blocking filesystem calls can delay cancellation. Audio matching remains unimplemented. |
| Retiming domain operation | One validated batch edit preserves cue text/identity/order and supports atomic Undo/Redo; [ownership walkthrough](resynchronization.md). Rust and modeled-browser tests cover proposal ownership, Save As/recovery and project references. The actual Mac native workflow passes review/accept/Undo/Redo, source-path rejection, save-before-project and reopen. | Native resync dialogs were exercised on Mac only. Recovery tests do not imply power-loss guarantees or restoration of unapplied drafts/history. |
| Generated IPC | All commands/events use one Rust registry and pinned Tauri Specta bindings behind the client facade. Checked integer/finite-float boundaries and actual Serde samples cover project flattening/defaults and DTO shapes. CI checks drift. Real default-app IPC passes on all three local ARM64 OSes; Mac native resync transport also passes. | The pinned generator has documented naming limitations; upgrades require contract and runtime regression checks. |
| Native playback | Narrow Mac ANGLE/OpenGL, Windows D3D11 and Linux GTK adapters share transport policy while retaining platform-required rendering ownership. Pinned source recipes, SDK preflight and build/runtime receipts are available. | Native configurations remain experiments. Candidate audio/content, device clocks, composition, lifetime and distribution gates remain in the [native contract](native-runtime.md#remaining-adoption-gates). |
| Automation and distribution | All ten [application CI jobs](https://github.com/golgeek/visualsubforge/actions/runs/38331827755) and six [native build-contract jobs](https://github.com/golgeek/visualsubforge/actions/runs/38331827700) pass at `339e51e`, including the corrected 32-case browser suite. ARM64 and Intel macOS candidates pass bundled CLI checks, Developer ID signing and Apple notarization; their audited minimums are macOS 14.0 and 15.0 respectively. Their downloaded native/application sources and signed packages are hash-verified. The Ubuntu 24.04 x64 candidate passes clean-container installation, offline CLI and native smoke with measured virtual-audio readiness; its matching source/inventory audit is complete. GPL-3.0-or-later is selected for original code. | `v0.0.1-alpha` is not published and must include macOS, Windows and Linux packages. Windows installer, offline CLI, DLL unload and downloaded source/notice audits pass; the original hosted GUI run failed without an audio device. [Replacement validation](https://github.com/golgeek/visualsubforge/actions/runs/38233864612) passes the same package with a virtual WASAPI endpoint and software D3D11. A reproduced Linux cold-null-sink startup delay and its readiness control are documented in the release evidence. CI success does not establish GPU/audio or clean-install acceptance. See [release evidence](releases.md). |

## Latest resync evidence

Algorithm `scene-visual-edges-v4` adds actual first/final decoded frame evidence
for otherwise ambiguous first/last shots, while preserving the cut-derived fit.
Every intervening observation must support it; matching edge pictures or equal
duration alone do not establish continuity. On the development Mac, the combined
29-case extended/robustness corpus passes. Periodic-motion coverage rises from
77.8% to 93.3%, above the unchanged 90% target, with zero measured accepted error.
Whole-cycle deletions in the first/last shot remain unresolved; same-duration
120 ms replacements do not get bridged. Both reference-film corpora still pass
(animated coverage 97.9%; live action 96.6%, worst accepted endpoint error 5.385 ms).
The 165 ordinary workspace tests, strict Clippy and twelve explicitly invoked
FFmpeg integrations pass. The new EOF tests cover short clips, nonzero origins
and a longer audio track that supplies no video-edge evidence. At `921181c`,
Ubuntu/Windows ARM64 pass 167/160 ordinary tests, the same twelve integrations,
all 29 combined cases, three evaluator-oracle tests and strict Clippy. All 369
tracked files are hash-verified. The `ddc841a` Mac native bundle subsequently
passed proposal review/acceptance, one Undo/Redo, cancelled Save As, explicit
source-path rejection, save-before-project and reopen from an empty workspace.
Exact output bytes and all input hashes match expectations; the project refers
to the new SRT. The app quit normally. The [native walkthrough](resynchronization.md#native-resync-walkthrough)
records executable/runtime identities and retained screenshots. This closes
the first resync milestone; video-only fixtures do not validate audio. The
[real desktop workflow lane](../tests/desktop/README.md) now passes on Mac,
Ubuntu (Xvfb) and Windows ARM64, with actual webview/IPC, editing/history, failed/cancelled
saves, exact-byte reopen, process-loss recovery, external-file conflict protection
and normal shutdown. Its optional pinned WebDriver instrumentation is absent from
ordinary builds; only file choices and app-data location are supplied. CI
now also passes this lane on all three hosted desktop OSes at `a222eda`; see
[release evidence](releases.md) for run links and packaging limits.
The pinned test client is now WebdriverIO 10.0.2, and source-map-js is patched to
1.2.2; a dated npm audit reports zero advisories. A new
private IO seam now tests partial-write and sync failures on actual temporary
files, preserving existing/absent destinations and cleaning temporaries; the
111 application/desktop tests and strict Clippy pass on Mac; both preparation
tests also pass on Ubuntu and Windows ARM64.
These sampled heuristics
do not guarantee detection of visually indistinguishable edits.

The following results describe the preceding committed v3 implementation:

Algorithm `scene-visual-neighborhoods-v3` adds bounded dense target
observations around unsupported intervals. A newly generated deletion of one
repeating motion cycle exposed a 1,760 ms false accepted mapping in v2; v3 leaves
that tail unresolved. All original 16 cases pass, with reference coverage 97.9%.
The expanded 21-case robustness corpus passes 20 cases: the nonrepeating pan
reaches 92.9% and its tail-deletion variant 95.7%, but periodic-motion coverage
remains below its 90% gate at 77.8%. These cases have no measured wrong accepted
timings. Live-action coverage reaches 96.6%, with a 5.385 ms worst accepted
endpoint error. An intermediate 80.9% regression exposed an interval merge that
mixed two fits' endpoints and invented a third slope. Retaining an original
cut model fixes the regression; its failing unit test and corpus evidence are
preserved. Latest local workspace tests (162), strict Clippy and ten explicitly
invoked visual/shared-operation/CLI/desktop-owner FFmpeg tests pass. At `f3df34c`,
Ubuntu ARM64 passes 164 ordinary workspace tests and Windows ARM64 passes 157;
both pass the same ten FFmpeg integrations, all 16 regenerated extended cases
and strict Clippy. Source hashes match all 368 tracked files. The Windows
filter-file test exceeds its command-line limit without putting the filter on
that command line. Mac Ctrl-C during actual dense sampling reaps the child,
removes both filter files and publishes nothing. Native GUI validation remains.

The following results describe the preceding committed v2 implementation:

The sampled verifier now preserves cut-derived evidence on either side of
differing timing models and leaves their uncertain gap unresolved. The original
16-case corpus still passes, with reference coverage 94.3% and zero measured
accepted error. A new low-cut/short-deletion experiment improves from 50.6% to
85.3% coverage but still misses its 90% target. A separate nonrepeating pan
reaches 85.6%; inspection shows that half-second sampling can miss the matching
frame at the predicted timestamp. The held-out live-action speed/ad edit reaches
95.8% coverage with 5.385 ms worst accepted error, and an actual three-cue CLI
run preserves inputs and publishes correct output. Matching quality remains an
open gate; see the [corpus record](../tests/fixtures/resync/README.md).

The [dated resync validation record](resynchronization.md#recorded-resync-validation)
distinguishes the current macOS, Ubuntu ARM64 and Windows ARM64 results. Both VM
headless suites, actual FFmpeg operations and ten compact alignment cases pass.
Both VMs' desktop owner/persistence, generated IPC and frontend checks pass.
Windows console Ctrl-C and timeout terminate the analysis child and preserve
files. Native GUI resync and the broader matching corpus were still open at
that earlier stage; the v4 and native-walkthrough results above supersede it.

## Earlier implementation evidence

The table records review-remediation results, not a fresh-machine installation or a
claim that all platform gates passed. Commands are in [CONTRIBUTING](../CONTRIBUTING.md)
and the [native contract](native-runtime.md).

| Check | Recorded result and scope |
| --- | --- |
| Rust workspace | 99 tests passed on macOS arm64, including selected `macos-angle` execution. Eleven opt-in FFmpeg integrations are separate; an ignored subprocess helper is executed by its owning test. Empty/doc-test harnesses are not feature tests. |
| Frontend | 39 TypeScript tests; strict Svelte/TypeScript, lint, formatting and production build passed. |
| Browser | 28 Playwright cases passed on Chromium/WebKit: nine workflow pages, four runner controls and a delayed-listener control per engine. The pages execute 158 component assertions per engine; the delayed-listener control separately repeats 26 project assertions. |
| Media analysis | Four waveform and seven scene integrations passed with Homebrew FFmpeg 9.0.2 and generated fixtures. |
| Native synthetic controls | 66 Python tests across ten suites and the fake-mpv C++ policy test passed. This validates test oracles and policy, not captured audio or device playback. |
| Pinned mpv variants | Fresh render/baseline/candidate builds passed 37/39/39 upstream tests, without skips. Dependencies were recorded, not hermetically locked. |
| Selected Mac application | Fresh Cargo target and bundle built; native-feature Clippy passed. Existing ANGLE libraries plus restored pinned headers were used. M3 Max visible video, GPU command smoke, expected loaded-library identities and normal quit passed; no fresh ANGLE library build or acoustic acceptance. |
| Native compiler checks | Mac ANGLE, Windows 11 ARM64 MSVC and Ubuntu 26.04 ARM64 C++ checks passed with pinned mpv headers; Linux Rust native-boundary check also passed. These are compiler checks, not new Windows/Linux playback evidence or x64 CI execution. |

Subsequent resync experiment checks on the development Mac: 23 media-analysis
unit tests and strict Clippy passed, plus all three visual integrations. The
four waveform and seven scene integrations passed in the preceding increment.
All 16 extended/reference corpus cases pass: reference coverage rose from 82.8%
to 95.2% through sampled verification, held-out speed/ad coverage is 99.2%, and
the long-block case reaches 100%. Accepted intervals have zero measured timing
error; static-deletion tails remain unresolved. These results do not establish
general content/distortion quality or other-platform behavior.

Shared extraction: `cargo test --locked -p vsf-application -p visualsubforge`
passed 73 tests (70 desktop, three moved persistence/lease tests); the save-lock
helper is invoked by its owner. Strict Clippy for both packages and rustfmt passed.
This preserves existing behavior; it does not validate a product CLI yet.

Shared resync operations: 15 application tests and all 70 desktop tests passed,
including process-crash/report-only publication, collision preservation, rounding,
hash changes and cancellation. Three additional actual-FFmpeg integrations passed
(offset/FPS/speed-ad mapping, unresolved output, changed inputs and cancellation).
The 23 media-analysis tests and strict Clippy for all three affected packages
passed. Publication behavior is locally verified on macOS; OS CI execution and
the product adapters remain outstanding.

CLI executable checks on macOS: three default tests and two opt-in FFmpeg tests
passed, including JSON stdout separation, report/SRT round trips, unresolved
output refusal, retry protection, actual SIGINT cancellation and active-child
reaping on cancellation/timeout. Strict CLI Clippy passed. A headless OS matrix
is configured but not run here; Windows console signals remain unverified.

Historical exact-gap refinement (superseded by the content-timing correction below): eight proposal tests passed, including 32-cue sequences at
46 speed scales, unsorted SRT order, Undo/Redo, ad-boundary exceptions and collapse
refusal. Three application FFmpeg tests and two CLI FFmpeg tests pass with an
actual 100 ms source gap preserved at 24/25 speed and a separate ad gap left to
the section mapping. These validated the then-approved end adjustment; current resync instead maps
every endpoint directly, as documented below.

After the CLI and exact-gap changes, `cargo test --locked --workspace` passed
131 tests and strict workspace Clippy passed. The workspace command leaves
FFmpeg integrations opt-in; the three shared-operation and two CLI FFmpeg tests
were run separately and passed. Formatting and diff checks passed. This is
macOS default-build evidence, with no new native GPU/audio or other-OS claim.

Generated IPC first slice: timing preset command/DTO generation passes with
Tauri Specta/Specta 2.0.0-rc.25 and specta-typescript 0.0.12. The 70 desktop tests,
strict desktop Clippy, 39 TypeScript tests, strict Svelte/TypeScript, lint,
formatting and production build passed. All 28 Chromium/WebKit cases passed;
the timing workflow additionally checks serialized string rejection and null
success through the facade. Deterministic `npm run ipc:check` passes; a deliberate
drift control fails without rewriting the modified file. This is a first-family
trial, not completion of the remaining enum/event/project/numeric gates.

Document IPC migration: tagged edit commands, accepted snapshots, recovery and
quit now use generated bindings. Checked u64 codecs reject values outside the
JavaScript exact-integer range; a recovery listing retains usable SRTs even when
old informational metadata cannot be represented. Module-qualified generated
names avoid the pinned generator's container-rename/phase collision; the facade
keeps readable aliases. `cargo test --locked -p visualsubforge` passed 74 tests
plus the separately invoked exporter. Strict desktop Clippy, frontend build,
all 28 Chromium/WebKit cases, binding drift, lint and formatting checks passed.
Four added boundary regressions cover scalar and field codecs and recovery
preservation; generated samples cover actual Serde snapshots and all document
action variants. Other command families and project/event compatibility remain.

Full IPC migration: all 24 commands and the workspace-action event now share
one Rust dispatch/generation registry; the client facade has no handwritten DTOs
or command names. The compatibility trial found and repaired two issues: Serde
non-finite floats could become null, and two codec-bearing View structs collided
in the generator's phase output. Required numeric outputs now have checked
serializers; SceneView/WaveformView have distinct Rust names. Actual Serde
samples cover project flattening/defaults and scene/waveform/event shapes.

Validation on the development Mac: `cargo test --locked --workspace` passed 140
tests (79 desktop), strict workspace Clippy passed, and `cargo check --locked
-p visualsubforge --features media-prototype` passed. This last check is compilation,
not GPU/audio execution. `npm run build`, `npm run lint`, `npm test` (39), formatting
and `npm run ipc:check` passed. The first browser run passed 26/28: the delayed
listener controls blocked module loading after the generated runtime switched
to a static import. The controls now hold the modeled registration response;
`npm run test:browser -- --grep projects` passed all four affected Chromium/WebKit
cases, including the two formerly failing controls. `cargo tree --locked -p
vsf-cli --edges normal` confirms no Specta/Tauri/native playback dependency.
Cross-platform/native IPC execution remains part of final integration validation.

Content-timing correction (2026-10-11): the user clarified that every cue start
and end, including long pauses, should follow actual matched programme timing.
The superseded fixed-ms end adjustment is removed. Reports use schema 2 and
`content_mapped_endpoints_v1`, with original/resulting gaps and separate section
boundaries. FPS-only conversion is not treated as a speed change. Preset/minimum
gap enforcement was a separate follow-up (now implemented in the quality milestone);
audio matching remains deferred.

`cargo test --locked -p vsf-application -p vsf-cli` passed 22 tests, including
nine proposal tests. Generated sequences cover 256 cues at 46 scales with
independent endpoint expectations, short/long gaps, plus explicit 25→23.976
speed examples and reverse mapping. An initial round-trip assertion compared
un-normalized SRT trailing whitespace; it now compares parsed documents.
All three shared-operation and two executable FFmpeg tests passed via the
resync guide's opt-in commands. Strict application/CLI Clippy, rustfmt and diff
checks passed. This changes proposal timing/report policy, not visual matching;
the corpus need not be retuned. GUI integration and remaining platform/content
gates stay active.

## Historical evidence and local context

Earlier task records describe bounded Mac, Windows ARM64 VM and Ubuntu ARM64 VM
editing, recovery, preview and dialog trials. Those results apply to their dated
source/runtime/gesture combinations. They must not be reassigned to a newer
candidate or counted as release support. In particular, completed bounded
recovery trials and an unresolved overall adoption decision can coexist.

`AGENTS.md`, `START-HERE.md`, `docs/architecture.md`, `docs/product.md`,
`docs/decisions/`, `docs/tasks/`, `docs/reviews/` and legacy research are
intentionally local/untracked context. Historical experiment notes may link to
those files. Their absence from a fresh checkout is expected; they are not
required by the tracked setup, tests, SDK preflight or recipe driver. Read them
when present, preserve their historical results and do not execute old task
instructions merely because they describe unfinished work. Proposed architecture
decisions have not been silently promoted to accepted decisions.

## What comes next

The review's persistence, automation, frontend ownership and bounded-analysis
remedies are implemented. Retain their failure/order regressions as features grow.
The native maintenance contract is established; native adoption is still blocked
by its explicit content/clock/composition gates. Repeat affected evidence on the
exact runtime before accepting it, and run configured CI when a remote is available.

The shared application, resync, quality, text-editing, scene-quality,
duration/speed-repair and statistics milestones are complete within their recorded
scope. Loss-preserving ASS/SSA documents now also pass their full scoped audit,
including existing styles, format-aware tools and tagged-SRT regression. The active
[umbrella roadmap](feature-roadmap.md) has completed fonts, batch timing, ASS/SSA
resync, language tools, style/font authoring, conversion and WebVTT within their
recorded Mac scopes. Legacy project import is next, followed by manual translation,
bounded plugins and native/distribution reliability.
Broader matching evidence remains useful; retain conservative
refusal at ambiguous or missing content. Packaging, licensing, physical-device/x64
and scale/display coverage remain prerequisites for corresponding release claims.

Desktop resync backend validation (2026-10-11, development Mac, pinned Rust/Node,
FFmpeg 9.0.2): `cargo test --locked --workspace` passed 153 tests, including
91 desktop tests; zero-test harnesses are not feature evidence. The separately
invoked `cargo test --locked -p visualsubforge resync::tests::actual_analysis --
--ignored --nocapture --test-threads=1` passed, exercising actual FFmpeg, bounded
review pages, acceptance and changed-input rejection with controlled media
selection. `cargo test --locked -p media-analysis --test ffmpeg -- --ignored
--nocapture --test-threads=1` passed all four waveform integrations after bounding
its decode threads (two-hour fixture: 4.126 seconds). Strict Clippy for desktop
and media-analysis, Rust formatting, frontend build, IPC drift, lint, 39 frontend
unit tests and formatting passed. `cargo check --locked -p visualsubforge
--features media-prototype` passed; this is compilation, not native execution.

New deterministic tests cover drain/reuse/panic handling, no queued resync job,
cancellation and shutdown ownership, media-selection serialization, stale
captures, one-edit Undo/Redo, protected Save As, actual recovery publication,
legacy recovery reads, project-reference rejection and retained scene polling.
The CI desktop matrix now configures the actual-FFmpeg owner test on all three
OSes. No remote is configured, so no hosted run is claimed. GUI interaction,
real Tauri transport, wider corpus and remaining OS checks are still required.

GUI review increment (2026-10-11): Tools opens a focused resync dialog and
`ResyncSession`, with source/target roles, draft apply/discard/cancel, progress,
100-row cue/section pages, unresolved reasons and complete-proposal acceptance.
DocumentSession uses one serialized reconciliation path for ordinary actions
and resync acceptance. App adds modal occlusion, focus and dispatch only. Closing
while Start is pending cancels its eventual ID; old view/picker replies cannot
revive a disposed job. The dialog waits for draft/accept acknowledgment before
allowing Close, preventing late accepted replies from overwriting later typing.

`npm run test:browser` passed all 30 Chromium/WebKit cases, including the new
resync workflow. After adding explicit draft-discard and same-revision Save As
controls, `npm run test:browser -- --grep 'workflow: resync'` passed all 21 fixture
assertions on each engine. `npm run build`/`npm run check` passed without warnings;
IPC generation/drift, lint and formatting passed. Strict desktop Clippy and
`cargo test --locked -p visualsubforge` passed all 91 ordinary tests (explicit
exporter/FFmpeg tests remain opt-in). No matching algorithm changed.

Interactive modeled review was inspected at 1440×1000 and 480×700. The narrow
dialog stayed inside the viewport without outer horizontal overflow; tables
scroll separately, including PageDown after keyboard focus. Ignored screenshots
are `tests/fixtures/generated/resync/gui-review-{wide,narrow}.png`. These browser
checks model media, file dialogs and storage. Actual Tauri/native resync, remaining
platform/process/filesystem and broader corpus validation remain open.
