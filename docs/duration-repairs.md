# Duration and reading-speed repairs

Implemented and validated within the scope below (2026-10-17). This authoring
aid does not establish dialogue synchronization or platform release acceptance.

## Review workflow

Choose **Repair duration / speed…** above the cue list. Apply, discard or keep an
unapplied draft before starting. Choose **Selected cue** or **Whole document** and
create a preview. Only cues with accepted duration, CPS, RS or unavailable-speed
findings are targets; compliant cues are not normalized toward an ideal rate.

Review both endpoints, duration, CPS/RS, whole-document finding totals and explicit
skip reasons. Starts stay fixed; ends may extend or shorten. A scope or selected
cue change invalidates the captured preview. Acceptance includes all safe rows
across all pages, uses one Rust history operation and reruns checking. Saving is
separate. Undo/Redo, source-conflict checks and recovery/resync Save As protections
remain active. A preview, failed acceptance or an all-skipped plan changes nothing.

Scene buffers require current completed Standard or Reduced results. Missing,
running or partial analysis cannot be acknowledged away. Zero/zero buffers
explicitly disable the scene requirement. Completed scans remain heuristic,
particularly Reduced detail; this is the same coverage contract as scene review.

## Exact target policy

All calculations use integer milliseconds and the active profile's existing text
and reading-start policies. Let N be the counted text length, A=N*1000 and D the
effective reading duration:

| Enabled bound | Required effective duration |
| --- | --- |
| CPS maximum C, inclusive | D >= ceil(A/C) |
| CPS minimum C, inclusive | D <= floor(A/C) |
| RS maximum R, exclusive | D >= 501 + floor(A/R) |
| RS minimum R, inclusive | D <= 500 + floor(A/R) |

A requested CPS metric requires at least 1 ms of effective reading time; RS
requires at least 501 ms. Empty counted text has no speed target, matching the
checker. Enabled actual-duration bounds are inclusive and use the fixed cue
start. Intersect all enabled bounds with the domain time range; never clamp one
rule independently and silently leave another violated. No display rounding,
nominal FPS assumption or new ideal-speed threshold drives the result.

The proposed end cannot exceed the next chronological start minus the configured
minimum gap, even when gap warnings are disabled. This prevents new overlaps and
keeps separate original overlap groups independent. A final cue has only domain
and profile bounds. Scene exclusion interiors are forbidden; exact outer edges
are allowed, including touching windows. Crossing a cut alone remains valid.
The unchanged start can retain an existing scene finding; this action cannot
repair it. Choose the feasible end nearest the original, preferring the shorter
duration if equally close. A scene window can require its next permitted edge.

The overlap-aware reading start comes from the same shared function as checking.
Candidates run in stable chronological order, using earlier proposed timings.
A later target resolved by an earlier repair is shown without a direct edit.
Equal-start or nested groups remain manual. Timing-sensitive ASS effects, unknown
or malformed markup and individual cues over 64 KiB are also skipped. Static
supported tags, text, cue IDs, starts and source order remain unchanged.

Check the whole candidate against duration, speed, overlap, gap and scene rules,
including unchanged neighbors. Withhold an original overlap group's changes if
they add/worsen a timing finding or leave a changed cue's duration/speed unresolved;
then recheck the actual retained document. Independent safe groups can remain.
Conflicting profile/time bounds, neighbor limits, blocked scene ranges, unavailable
scenes and coupled cases have separate explanations. This deterministic review
plan does not search all possible joint retimings or infer dialogue synchronization
from reading metrics.

## Ownership and bounds

`vsf-application::quality::duration` owns the immutable baseline, before/after
reports and endpoint plan. It has no filesystem, Tauri, native-media or frontend
dependency. The shared checker supplies metrics and whole-document validation;
`Edit::Retime` validates the complete accepted document and records one history
entry. The desktop additionally enforces the SRT size/save policy before publishing.

`document::timing_repairs` owns one worker and global proposal IDs for both scene
and duration review. Purpose guards prevent cross-tool reads, acceptance or
cancellation. There is no pending queue, and the cooperative deadline is ten
seconds. Media selection -> scenes -> document locks protect capture/acceptance;
evaluation and shutdown joining happen outside the document mutex. Document,
profile, media and scene changes reject obsolete work. The checker/gap/text/timing
owners remain at most four independently bounded CPU workers.

The typed client exposes duration start/view/cancel/accept. `TimingRepairSession`
shares bounded polling, scope/snapshot guards and late-admission disposal for
scene/duration dialogs. `DocumentSession` alone reconciles accepted replies with
ordinary edits. Shared dialog code displays Rust results; it computes no repair
arithmetic. Review IPC caps at 200 rows and the UI renders 100 per page. Existing
100,000-cue/cut/finding and 16 MiB source-text ceilings remain; these are not latency
claims for every maximum-size input.

## Validation

Pinned Mac toolchains: Rust 1.98.1, Node 24.21.0/npm 11.19.0. Shared tests pass
49 cases (two ignored subprocess helpers are not counted). The new endpoint oracle
enumerates candidates using the independent checker. Boundary, scope, Unicode/tag,
ambiguous/timed markup, coupled speed, cancellation and atomic-history tests pass.
Eleven desktop timing-owner tests cover both tools, exact save/reopen, failed saves,
recovery/resync flags, wrong-kind IDs, invalidation, no-queue cancellation and retry.
All 255 ordinary workspace tests, 39 frontend unit tests and 44 Chromium/WebKit
cases pass. Thirteen duration assertions per engine cover draft protection, scope,
coverage, paging, acceptance failure, stale replies and disposal; these model IPC.
Strict Clippy, Svelte/TypeScript, lint, formatting, generated IPC drift and the
default desktop build pass. Empty/doc-test harnesses are not feature validation.

Before measurement, budgets were set to proposal p95 <500 ms over 20 warm release
runs, <96 MiB additional sampled RSS and <1 second desktop interaction/acceptance/
cancellation/invalidation. The 10,000-cue/940,000-byte fixture uses 10,000 cuts,
1400 ms spacing, duration 800–1200 ms, CPS 60–100, RS [5,500), gap 80 ms and scene
buffers 130/130 ms. Every extension and trim workload changes all 10,000 cues;
ordinary-profile safety has separate fixtures.

The first run passed CPU but failed RSS with 140,279,808 added bytes. Heap/VM
inspection after dropping proposals showed approximately 95 KiB live allocations
and zero detected leaks, with over 100 MiB retained in empty allocator regions.
Reserving initial finding capacity from the bounded cue count avoids the repeated
large growth pattern. The same unchanged workloads/budgets now pass at extension
p95 23.209 ms, trim p95 20.371 ms and 40,255,488 added RSS bytes. An already-sorted
fast-path experiment did not help and was reverted. Diagnostic instrumentation
was removed before the passing run. Rebuilt quality, text and scene benchmarks
also pass their original CPU/RSS budgets after the common allocation change:
quality p95 6.758 ms/10.1 MB added RSS; text p95 15.589–37.820 ms/22.2 MB;
scene check/repair p95 7.638/21.021 ms/24.4 MB. Raw regression JSON is retained.

The actual media-enabled Mac run passes selected/all previews, protected drafts,
static-tag preservation and a timed-tag skip, fixed starts, failed-save preservation,
exact BOM/CRLF save/reopen, one Undo/Redo and scene/media invalidation. The combined
run also passes the existing scene, gap, text, settings, CLI-parity, restart and
recovery workflows, then quits normally. Native composition uses the development
Homebrew mpv/OpenGL adapter; this is not adoption of the ANGLE/audio candidate.

The 10,000-cue desktop fixture mixes extensions and trims and verifies every
result, 100-row pages, whole-batch acceptance from page two and exact Undo. Debug
preview takes 418 ms, acceptance 113 ms, generated draft interaction 1 ms,
cancellation 3 ms and edit/result invalidation 13 ms. All declared interaction
budgets pass. Retained screenshots show before/after metrics, explicit skips,
scene coverage and second-page acceptance controls. Queued file choices and
webview-generated events are not physical input or native-dialog validation.

```sh
cargo test --locked -p vsf-application --lib
cargo test --locked -p visualsubforge timing_repairs
npm run test:browser -- --grep 'duration-quality|scene-quality|gap-review'
cargo build --locked --release -p vsf-application --example duration-benchmark
python3 scripts/duration-benchmark.py
cargo build --locked --release -p vsf-application --example quality-benchmark --example text-benchmark --example scene-benchmark
python3 scripts/quality-benchmark.py
python3 scripts/text-benchmark.py
python3 scripts/scene-quality-benchmark.py
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm test
npm run lint
npm run format:check
npm run ipc:check
npm run test:browser
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked -p vsf-cli
npm run test:desktop -- --scenes --duration
npm run tauri -- build --debug --no-bundle -- --locked
```

Receipts are retained under `tests/fixtures/generated/duration-quality/`, including
`benchmark-v1.json` (failed), `benchmark-v2.json` (passing), heap/VM controls,
`workspace-v1.log`, `browser-v2.log`, final check/build logs and `desktop-final/`
(real receipt, performance JSON, exact inputs/outputs and inspected screenshots).
RSS sampling measures this process, not complete allocation or native/webview
child memory. No Windows/Linux feature, installer or physical GPU/audio acceptance
is claimed. Full ASS/SSA documents/styles and statistics remain separate work.
