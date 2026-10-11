# Current implementation and validation status

Updated 2026-10-11 UTC, after review remediation through `6617275`.
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
| Native playback | Narrow Mac ANGLE/OpenGL, Windows D3D11 and Linux GTK adapters share transport policy while retaining platform-required rendering ownership. Pinned source recipes, SDK preflight and build/runtime receipts are available. | Native configurations remain experiments. Candidate audio/content, device clocks, composition, lifetime and distribution gates remain in the [native contract](native-runtime.md#remaining-adoption-gates). |
| Automation and distribution | Two workflows configure locked application checks, browser workflows, FFmpeg integrations, synthetic controls, mpv source builds and native compiler lanes. | No GitHub run is claimed: this checkout has no configured remote. No installer, signing or release dependency/license inventory is complete. |

## Latest local evidence

These are results from this remediation, not a fresh-machine installation or a
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

Broader formats, plugin frameworks, generated DTO infrastructure and cosmetic
module rearrangements can wait. Packaging, licensing, physical-device/x64 and
scale/display coverage remain prerequisites for corresponding release claims.
