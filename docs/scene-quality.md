# Scene-aware quality and timing repair

The scene-quality milestone is complete within the evidence below (2026-10-17).
This is an authoring aid over heuristic visual scene data; platform release and
physical GPU/audio acceptance remain separate.

## Endpoint policy

The active quality profile's existing `sceneBeforeMs` and `sceneAfterMs` values
define open exclusion windows `(cut - before, cut + after)`. Both cue endpoints
are checked. Exact outer boundaries are allowed, including a point where two
windows merely touch. Strictly overlapping windows merge. Negative left bounds
remain negative: a cut at zero with a positive before buffer can exclude time
zero. Zero/zero disables these checks. A cue crossing a cut has no finding unless
one of its endpoints lies inside an exclusion window.

The media adapter converts actual scene timestamps from seconds to integer
milliseconds once, rounding to nearest with positive ties upward, then removes
duplicates. No nominal frame rate is inferred from profile names. For example,
1001/24000 seconds becomes 42 ms. Source timestamps remain unchanged in caches.

Problems offers **Start near scene cut** and **End near scene cut** filters and
cue navigation that commits pending fields before selecting the target. Coverage
is separate from findings and profile limitations. Missing media, unavailable
analysis and partial/running scans are not passing scene results. Completed
Standard and Reduced scans can supply checks; both remain heuristic, and Reduced
can miss cuts. A completed empty scan is different from unavailable data.

Stopping a rescan or encountering a scan error restores the previous completed cuts for the same media session. The timeline, quality checks, timing repairs and project saves use that scan and its original detail level. The error remains visible after a failed rescan. A first scan can only show its partial results.

The quick pass uses half the available logical CPUs for verification workers.
The full pass uses one third for software decoder threads and one filter thread.
Both counts are rounded to the nearest integer and limited to 2–8.
On macOS/Linux the scan increases FFmpeg niceness by 5 when `/usr/bin/nice`
is available and uses normal priority otherwise. It uses below-normal priority on
Windows. Scheduling does not alter the Standard/Reduced detector settings or
their saved-project analysis versions (3/4).

## Reviewing shot changes

**Snap to shot changes…** opens the shot preset. The review applies the active profile’s exclusion windows and protects other timing rules. [Review workflow and coverage](timing-repair-all.md)

## Headless reports

This milestone introduced quality-report schema 2 with `sceneCoverage`.
Current language tools extend it to [schema 3](language-tools.md), retaining that field.
`complete` requires both complete profile coverage and available scene input
when buffers are nonzero. `vsf check` does not decode media or invent scene data;
it reports `noMedia` and exit 3 for such profiles, even with zero findings.
For intentionally text-only CLI checks, use a custom profile with both scene
buffers zero. This is an explicit coverage change from the previous checker.
The desktop uses `evaluate_with_scenes`; headless callers can supply the same
bounded, immutable scene input directly through the shared Rust API.

## Desktop timing preview at 10,000 cues

On 2026-11-25, build `49fd211` ran on an Apple M3 Max (Mac15,10, 36 GiB,
macOS 27.0 build 26A428) with Rust 1.98.1 and Node 24.21.0. The scene fixture
has 10,000 cues and 10,000 cuts. The scene stopwatch starts once WebDriver sees
the review panel and stops when it observes **Apply 10000 changes**. WebDriver
polls the visible control every 100 ms.

| Build | Scene preview, release (ms) | Scene preview, debug (ms) | Duration preview, release (ms) |
| --- | --- | --- | ---: |
| `0fc109e` | 207, 210, 208 | 1,250 | 113 |
| `49fd211` | 104, 110, 105 | 523, 520, 519 | 107 |

Both profiles use the 1,000 ms preview budget. The three release scene runs
and the release duration run passed through acceptance, Undo and invalidation
to normal shutdown. All three debug scene runs passed the timing checks,
then exceeded the separate release-build budget for the quality index.
Windows and Linux timings are unverified.

The release lanes use the macOS ANGLE build environment.

```sh
npm run tauri -- build --no-bundle --features desktop-e2e,macos-angle --config tests/desktop/tauri.test.json -- --locked
cargo build --locked --release -p vsf-cli
node tests/desktop/run.mjs --scenes --binary target/release/visualsubforge --cli target/release/vsf
node tests/desktop/run.mjs --duration --binary target/release/visualsubforge --cli target/release/vsf
```

The debug measurement uses the development media adapter.

```sh
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
node tests/desktop/run.mjs --scenes --binary target/debug/visualsubforge --cli target/release/vsf
```

## Historical validation

Development Mac arm64, Rust 1.98.1, Node 24.21.0/npm 11.19.0, FFmpeg 9.0.2.
The full Rust workspace passes 236 tests, excluding empty harnesses; 39 frontend
unit tests and 42 Chromium/WebKit cases pass. The scene workflow runs ten checks
per engine, including draft protection, unavailable/partial/reduced coverage,
all-skipped proposals, paging, failed acceptance and late/stale replies. Strict
Clippy, Svelte/TypeScript, lint, formatting, generated IPC and default desktop
build checks pass. The media-enabled instrumented Mac app also builds and runs.

Shared/desktop tests cover exact open boundaries, merged/touching windows,
zero/negative edges, fractional PTS rounding, unsorted/nested cues, whole-document
speed regressions, Unicode/static/timed/unknown tags, atomic Undo/Redo, failed
saves and recovery/resync protection. Worker tests cover no-queue admission,
active cancellation, late cancellation after scene replacement, failure/retry,
shutdown and document/profile/scene invalidation.

The actual Mac webview/IPC workflow restores synthetic projects with known cuts,
checks protected Problems navigation and preview acceptance, preserves static
ASS tags, skips karaoke, verifies failed-save preservation and exact BOM/CRLF
save/reopen, and exercises one Undo/Redo. An actual FFmpeg rescan invalidates an
old proposal; media replacement rejects another. Detaching media reports missing
coverage. All ordinary Settings/text/save/restart/recovery steps still pass.

Predeclared scale budgets: 10,000 cues/940,000 source bytes and 10,000 cuts;
release check/proposal p95 <500 ms over 20 warm runs; additional sampled RSS
<96 MiB; desktop interaction/cancellation/invalidation <1 second. Final release
results at that checkpoint were 7.124 ms check p95, 22.531 ms repair p95
and 37,257,216 added RSS bytes. The stress profile permits all 10,000 repairs.
Ordinary profile safety has independent correctness fixtures.

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm test
npm run lint
npm run format:check
npm run ipc:check
npm run test:browser
npm run tauri -- build --debug --no-bundle -- --locked
cargo build --locked --release -p vsf-application --example scene-benchmark
python3 scripts/scene-quality-benchmark.py
npm run tauri -- build --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked --release -p vsf-cli
npm run test:desktop -- --scenes --binary target/release/visualsubforge --cli target/release/vsf
```

The initial workspace run exposed two CLI assertions assuming full coverage
without scenes. Updated assertions and an explicit zero-buffer passing control
now verify schema 2 and exit 3; the complete rerun passes. A benchmark wrapper
initially reused the existing scene-analysis script's name; the original script
is preserved and the new wrapper is `scene-quality-benchmark.py`.

Ignored evidence is under `tests/fixtures/generated/scene-quality/`: the full
workspace log `workspace-v1.log`, final check logs, `benchmark-final.json`,
`artifact-identities.json`, and `desktop-final/` with receipts, synthetic inputs,
outputs and screenshots. The native run uses the Mac OpenGL development adapter
and Homebrew libmpv (pkg-config 2.5.0), not the ANGLE candidate. Screenshots were
inspected for review rows and accessible acceptance/paging controls. Generated
webview events and supplied file choices do not measure physical input or native
dialogs. Known-cut caches are not cut-detection accuracy evidence. RSS samples
measure the proposal process, not WebKit/native child memory or all allocations.
Full ASS/SSA document/style editing, Windows/Linux feature execution, installation
and physical GPU/audio acceptance remain separate milestones.

## Historical status receipt: Scene-quality milestone complete (2026-10-17)

Relocated from the status page. This receipt retains the scope and pending
work at that checkpoint; subsequent evidence and the [current status](status.md)
supersede its staging statements. Moving it adds no validation claim.

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
invalidation and the existing restart/recovery workflow. Quality report
schema 2 adds scene coverage: the CLI returns incomplete coverage for nonzero
scene buffers because it supplies no scene data. See [scene quality](scene-quality.md)
for exact commands, retained evidence and the Mac OpenGL runtime scope. New
Windows/Linux feature execution and physical GPU/audio acceptance are unverified.

With attached video, an incomplete or unavailable scene scan blocks every timing change. Finish the scan before applying repairs. With no video, only Shot changes is disabled.
