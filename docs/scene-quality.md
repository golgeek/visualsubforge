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
the existing protected cue navigation (Apply/Discard/Cancel for drafts). Coverage
is separate from findings and profile limitations. Missing media, unavailable
analysis and partial/running scans are not passing scene results. Completed
Standard and Reduced scans can supply checks; both remain heuristic, and Reduced
can miss cuts. A completed empty scan is different from unavailable data.

## Review and acceptance

**Repair scene timing…** creates an immutable preview. Starts move inward to the
right edge of their exclusion window; ends move inward to the left edge. No cue
is extended or shifted. Text, tags, IDs and document order are preserved. The
dialog shows both endpoints, duration, CPS/RS, total before/after findings and
specific skip reasons in pages of 100 rows (IPC maximum 200).

Nested/equal-start overlap groups, nonpositive intervals, enabled duration-limit
failures and ambiguous markup are skipped. Karaoke, transforms, move/fade effects
and unknown/incomplete tags require manual timing review; static supported tags
remain byte-identical. Individual cues over 64 KiB also require manual review.
The shared source-mapping parser is used by both text tools and timing repairs.

The entire proposed document is checked for new or worsened duration, speed,
overlap, gap and scene findings, including effects on unchanged overlapping cues.
Unsafe overlap groups are withheld and the retained document is checked again.
Existing unrelated findings are reported; acceptance does not claim universal
compliance. Loading a profile never authorizes automatic edits.

Accept applies every safe row across all pages as one Rust history transaction.
One Undo/Redo restores the full batch. Saving remains separate; atomic-save,
external-change, recovery and resynchronization Save As protections remain active.
Scene repair requires current completed scene data; missing data cannot be
acknowledged away. An unchanged or all-skipped proposal cannot create history.

## Ownership and bounds

Pure scene inputs, checks and proposals live in `vsf-application::quality::scene`.
No native media, filesystem, Tauri or frontend types enter that module. Accepted
state/history remains in Rust. The frontend owns only transient review, navigation
and draft state; `DocumentSession` serializes acceptance with other edits.

Quality reads capture media/scene/document context off the UI thread. Scene
replacement invalidates checker results even without a subtitle revision change.
Proposal reads/acceptance recheck the document revision, profile epoch, media
session and scene generation/readiness. Lock order is media selection, scenes,
then document. Scene scan/restore inputs must match the selected media path.

The scene review owner has one worker, no pending queue and a ten-second deadline.
Cancellation is cooperative throughout checks and proposals; shutdown joins
outside the document mutex. It joins the existing independently bounded quality,
gap and text workers (at most four CPU workers). All work is bounded by 100,000
cues/cuts/findings and 16 MiB of subtitle text. These are ceilings, not measured
responsiveness claims at every ceiling.

## Headless reports

Shared quality reports now use schema version 2 and include `sceneCoverage`.
`complete` requires both complete profile coverage and available scene input
when buffers are nonzero. `vsf check` does not decode media or invent scene data;
it reports `noMedia` and exit 3 for such profiles, even with zero findings.
For intentionally text-only CLI checks, use a custom profile with both scene
buffers zero. This is an explicit coverage change from the previous checker.
The desktop uses `evaluate_with_scenes`; headless callers can supply the same
bounded, immutable scene input directly through the shared Rust API.

## Recorded validation

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
results are 7.124 ms check p95, 22.531 ms repair p95 and 37,257,216 added RSS bytes.
The explicit stress profile permits all 10,000 repairs; ordinary profile safety
has independent correctness fixtures. The real debug app previews all 10,000
changes in 413 ms, accepts from page two in 110 ms and restores all with one
Undo. Generated draft interaction takes 1 ms, cancellation 3 ms and invalidation
13 ms; the review mounts 100 rows. Declared budgets pass.

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
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked -p vsf-cli
npm run test:desktop -- --scenes
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
