# Current implementation and validation status

Updated 2026-10-11 (America/New_York): headless resync is implemented; generated IPC and GUI integration are in progress.
This is the tracked summary of current behavior and evidence. The code is being
evolved into the product; experimental native playback does not make persistence
or maintainability disposable. There is no release or daily-use acceptance yet.

## Implementation, checks and limits

| Area | Implemented foundation and reproducible checks | Remaining limit or gate |
| --- | --- | --- |
| Subtitle editing | Rust owns accepted UTF-8 SRT, cue identity, generation/revision and undo/redo. Text, timing, create/delete/split/merge and format preservation have domain/application regressions. | SRT only; frontend drafts are not accepted state. Native preview has a separate 4 MiB limit. |
| Saves and recovery | Acceptance checks the shared 16 MiB persistence limit. Atomic saves compare prior bytes under a stable sibling lease. Conflicting VSF writers preserve the losing edit. Background recovery uses instance leases and stale-checkpoint guards. | External editors need not honor advisory locks. Recovery excludes unapplied drafts, undo history and edits since the last durable checkpoint; no power-loss guarantee. Windows/Linux execution of the newly added save-lease regressions is still unverified locally. |
| Projects and settings | Rust owns project attachment tokens and byte baselines; incomplete restore cannot attach. Quit offers retry/copy/cancel/explicit exit after project-save failure. Settings apply only after successful validated persistence. | Project metadata is separate from SRT/recovery. Settings retain last-successful-write behavior across instances. New quit-failure choices have modeled browser coverage; real OS close/modal coverage remains. |
| Frontend ownership | Scoped document, media and project owners retain projection/draft/response ordering and explicit disposal. One typed IPC client, pure timeline calculations and nine component/lifecycle pages have automated checks. | Modeled IPC/synthetic events do not establish native input, focus, dialogs or presentation. Already-dispatched Rust work can finish after frontend disposal. |
| Analysis | Waveform and visual scene analysis use cancellable FFmpeg processes, generation guards, one worker plus one pending request per service, and coalescing. Four waveform and seven scene integrations run actual FFmpeg. | Cancellation bounds admitted jobs, not the latency of blocking filesystem IO. Scene cuts are heuristic; Reduced detail can miss cuts. |
| Resynchronization experiment | Independent edit-list fixtures, bounded scene-signature matching, sampled verification and a headless evaluator in `media-analysis`; [commands and criteria](../tests/fixtures/resync/README.md). | All 16 current cases pass, including one real-content edit and long/held-out cases. Broader content/distortion evidence remains open. Generated IPC migration is in progress; GUI resync is not yet implemented. |
| Shared application and CLI | `vsf-application` owns shared SRT/persistence policies, immutable resync proposals, bounded/cancellable FFmpeg orchestration, content identities, reports and no-replace output publication. `vsf resync` calls it directly without desktop/native playback dependencies. Original millisecond gaps within a supported block survive speed changes by adjusting preceding ends. | GUI caller is next. Broader matching, OS/process/filesystem and GUI lifecycle gates remain. Bulk minimum-gap enforcement is a separate unimplemented editor action. |
| Retiming domain operation | One validated batch edit preserves cue text/identity/order and supports atomic Undo/Redo; [ownership walkthrough](resynchronization.md). | GUI Save As protection, proposal acceptance and lifecycle integration are not implemented yet. |
| Generated IPC | Timing preset, document, recovery and quit contracts are generated from pinned Tauri Specta, with checked document integers and the client facade preserved. Rust Serde samples are checked by TypeScript and desktop CI checks generation drift. | Remaining command/event families, project flatten/default phases and full compatibility evaluation are in progress. |
| Native playback | Narrow Mac ANGLE/OpenGL, Windows D3D11 and Linux GTK adapters share transport policy while retaining platform-required rendering ownership. Pinned source recipes, SDK preflight and build/runtime receipts are available. | Native configurations remain experiments. Candidate audio/content, device clocks, composition, lifetime and distribution gates remain in the [native contract](native-runtime.md#remaining-adoption-gates). |
| Automation and distribution | Two workflows configure locked application checks, browser workflows, FFmpeg integrations, synthetic controls, mpv source builds and native compiler lanes. | No GitHub run is claimed: this checkout has no configured remote. No installer, signing or release dependency/license inventory is complete. |

## Latest local evidence

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

Exact-gap refinement: eight proposal tests pass, including 32-cue sequences at
46 speed scales, unsorted SRT order, Undo/Redo, ad-boundary exceptions and collapse
refusal. Three application FFmpeg tests and two CLI FFmpeg tests pass with an
actual 100 ms source gap preserved at 24/25 speed and a separate ad gap left to
the section mapping. These validate the approved end adjustment, which can
intentionally differ from the direct content-mapped cue end.

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
and desktop resynchronization, with generated JSON IPC bindings. Continue the
application integration and broaden matching evidence; preserve conservative refusal
at ambiguous or missing content. Broader formats, plugin frameworks and cosmetic
module rearrangements can wait. Packaging, licensing, physical-device/x64 and
scale/display coverage remain prerequisites for corresponding release claims.
