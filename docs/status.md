# Current implementation and validation status

Updated 2026-10-11 (America/New_York): the first CLI/GUI resync milestone is complete, including generated IPC, local three-OS automated validation and the Mac native workflow.
This is the tracked summary of current behavior and evidence. The code is being
evolved into the product; experimental native playback does not make persistence
or maintainability disposable. There is no release or daily-use acceptance yet.

## Implementation, checks and limits

| Area | Implemented foundation and reproducible checks | Remaining limit or gate |
| --- | --- | --- |
| Subtitle editing | Rust owns accepted UTF-8 SRT, cue identity, generation/revision and undo/redo. Text, timing, create/delete/split/merge and format preservation have domain/application regressions. | SRT only; frontend drafts are not accepted state. Native preview has a separate 4 MiB limit. |
| Saves and recovery | Acceptance checks the shared 16 MiB persistence limit. Atomic saves compare prior bytes under a stable sibling lease. Conflicting VSF writers preserve the losing edit. Background recovery uses instance leases and stale-checkpoint guards. Local Mac/Ubuntu/Windows ARM64 suites and actual default-app save/recovery workflows pass. | External editors need not honor advisory locks. Recovery excludes unapplied drafts, undo history and edits since the last durable checkpoint; no power-loss guarantee. |
| Projects and settings | Rust owns project attachment tokens and byte baselines; incomplete restore cannot attach. Quit offers retry/copy/cancel/explicit exit after project-save failure. Settings apply only after successful validated persistence. | Project metadata is separate from SRT/recovery. Settings retain last-successful-write behavior across instances. New quit-failure choices have modeled browser coverage; real OS close/modal coverage remains. |
| Frontend ownership | Scoped document, media and project owners retain projection/draft/response ordering and explicit disposal. One typed IPC client, pure timeline calculations and nine component/lifecycle pages have automated checks. | Modeled IPC/synthetic events do not establish native input, focus, dialogs or presentation. Already-dispatched Rust work can finish after frontend disposal. |
| Analysis | Waveform and visual scene analysis use cancellable FFmpeg processes, generation guards, one worker plus one pending request per service, and coalescing. Four waveform and seven scene integrations run actual FFmpeg. | Cancellation bounds admitted jobs, not the latency of blocking filesystem IO. Scene cuts are heuristic; Reduced detail can miss cuts. |
| Resynchronization | Independent edit-list fixtures, bounded scene-signature matching, sampled verification and a headless evaluator in `media-analysis`; [commands and criteria](../tests/fixtures/resync/README.md). The v4 verifier passes 29 synthetic cases on Mac/Ubuntu/Windows ARM64 and both reference films on Mac. | Coverage is measured, not universal. Sampled visual evidence can miss indistinguishable edits; ambiguous or unsupported cues remain unresolved. Wider real-programme evaluation is a follow-up. |
| Shared application and CLI | `vsf-application` owns shared SRT/persistence policies, immutable resync proposals, bounded/cancellable FFmpeg orchestration, content identities, reports and no-replace output publication. `vsf resync` calls it directly without desktop/native playback dependencies. All cue endpoints and within-section gaps follow measured content timing; nominal FPS alone never determines speed. | Blocking filesystem calls can delay cancellation. Hosted/x64 CI remains unavailable. Bulk minimum-gap enforcement and audio matching are separate unimplemented features. |
| Retiming domain operation | One validated batch edit preserves cue text/identity/order and supports atomic Undo/Redo; [ownership walkthrough](resynchronization.md). Rust and modeled-browser tests cover proposal ownership, Save As/recovery and project references. The actual Mac native workflow passes review/accept/Undo/Redo, source-path rejection, save-before-project and reopen. | Native resync dialogs were exercised on Mac only. Recovery tests do not imply power-loss guarantees or restoration of unapplied drafts/history. |
| Generated IPC | All commands/events use one Rust registry and pinned Tauri Specta bindings behind the client facade. Checked integer/finite-float boundaries and actual Serde samples cover project flattening/defaults and DTO shapes. CI checks drift. Real default-app IPC passes on all three local ARM64 OSes; Mac native resync transport also passes. | The pinned generator has documented naming limitations; upgrades require contract and runtime regression checks. |
| Native playback | Narrow Mac ANGLE/OpenGL, Windows D3D11 and Linux GTK adapters share transport policy while retaining platform-required rendering ownership. Pinned source recipes, SDK preflight and build/runtime receipts are available. | Native configurations remain experiments. Candidate audio/content, device clocks, composition, lifetime and distribution gates remain in the [native contract](native-runtime.md#remaining-adoption-gates). |
| Automation and distribution | Two workflows configure locked application checks, browser workflows, FFmpeg integrations, synthetic controls, mpv source builds and native compiler lanes. | No GitHub run is claimed: this checkout has no configured remote. No installer, signing or release dependency/license inventory is complete. |

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
configures this lane on all three desktop OSes; hosted execution remains pending.
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
gap enforcement and audio matching remain separate follow-ups.

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

The active direction is shared Rust application operations for a headless CLI
and desktop resynchronization, with generated JSON IPC bindings. Complete native
workflow validation and broaden matching evidence; preserve conservative refusal
at ambiguous or missing content. Broader formats, plugin frameworks and cosmetic
module rearrangements can wait. Packaging, licensing, physical-device/x64 and
scale/display coverage remain prerequisites for corresponding release claims.

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
