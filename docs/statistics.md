# Document statistics

Implemented and validated on the development Mac (2026-10-17). The report is
read-only; full ASS/SSA documents/styles and platform release gates stay separate.

## Review

Choose **Statistics…** above the cue list. The report describes accepted subtitles;
unapplied drafts stay unchanged and do not participate. Choose the whole document
or an inclusive range of one-based document cue positions, optionally excluding
comma-separated positions inside that scope. These are not SRT sequence labels.
Duplicate exclusions coalesce. Invalid/out-of-range choices fail explicitly.
Empty documents and all-excluded scopes have usable empty reports.

Counts separate total, in-scope, outside-scope, explicitly excluded, included and
empty-text cues. Each duration, CPS, RS, line-count, longest-line and signed-gap
summary shows its sample count, minimum, maximum and mean. Extremum links choose
the first cue in document order on ties. All nine reading-speed categories are
shown, including zero counts. Outliers list existing quality findings and navigate
through the editor's Apply/Discard/Cancel draft guard. An explicit Apply there is
an ordinary document edit; calculating, filtering and closing statistics never
commits or discards anything. Save/recovery protections and history stay intact.

## Counting and denominators

Rust evaluates the whole document using the active quality profile before filtering
statistics. Overlap-aware reading starts and original chronological neighbors stay
unchanged at range boundaries or around exclusions. Text counting reuses the
existing UTF-16/tag/CRLF policy, not a second parser or frontend calculation.
The literal text `0` is nonempty. Empty counted text still contributes to duration
and line summaries but supplies no finite speed.

Arithmetic means weight each finite cue equally (each eligible pair for gaps).
Unavailable CPS/RS are omitted only from their own metric, with separate empty-
text, invalid-duration and reading-allowance counts. No infinity or replacement
zero enters a mean. Zero-sample means/extrema and zero-denominator percentages are
null. RS distribution percentages divide by finite RS count; boundaries and
classification are the existing Rust ratings, independent of display rounding.
Line summaries describe each cue's line count and longest stripped line, not
individual-line averages. Metrics remain visible when warning rules are disabled.

Signed gaps use original consecutive chronological pairs only when both cues are
included. Zero gaps and negative gaps count; exclusions never create a new neighbor.
The report separates negative/touching/positive counts and excluded original pairs.
A separate interval sweep counts all unordered overlapping pairs among included
cues and every included cue involved, including nested/equal-start intervals.
Touching is not overlap. This all-pair count differs from adjacent negative gaps
and from the quality checker's representative overlap findings.

Outliers are findings whose primary cue is included. Related cues remain available
as context, including outside scope; the affected-cue total includes only included
cues referenced by those findings. Profile and scene coverage remain explicit.
A useful statistical report does not imply complete quality coverage or accurate
speech synchronization. Completed visual scene detection is still heuristic.

## CLI

```sh
vsf stats --subtitles episode.srt --profile vsf-default --json
vsf stats --subtitles episode.srt --profile ./custom.vsfpreset --from-cue 10 --to-cue 100 --exclude 12,17 --json
```

Without `--json`, output is human-readable. Both range endpoints are required.
JSON contains a version-1 `summary` and all scoped `findings`, using the exact
shared report projected by the desktop. Output streams to stdout; diagnostics go
to stderr. Inputs and profiles are never modified. No GUI settings, media decoder,
or FFmpeg process is used. Consequently nonzero scene buffers produce no-media
coverage; use an explicitly zero-buffer profile when that is intended.

Exit 0 means a report with complete requested quality coverage, 3 partial coverage,
2 invalid input/resource limit, 5 IO and 130 cancellation. Findings do not make
statistics fail; use `vsf check` for quality-check exit semantics. The cooperative
calculation deadline is ten seconds; blocking filesystem IO cannot be interrupted.

## Ownership and bounds

`vsf-application::quality::statistics` consumes a shared checker report, retains
its profile/hash/coverage and aggregates selected metrics. It has no filesystem,
Tauri, native-media or UI dependency. `vsf stats` formats that report directly.
The interval sweep is O(n log n), without enumerating quadratic overlap pairs.

`document::statistics` owns one explicit worker with no queue and a ten-second
deadline. Admission captures an immutable document/profile/scene context; evaluation
runs outside locks. Only summary and findings remain afterward. Reads revalidate
document revision, profile epoch, media session and scene generation/readiness.
Lock order remains media -> scenes -> document; shutdown joins outside the document
mutex. This is a fifth independently bounded CPU worker alongside checker, gap,
text and shared timing review; it does not interrupt their existing jobs.

`StatisticsSession` owns only transient scope/capture/paging, one in-flight read
and late-reply disposal. Scope/workspace changes retire old results. Draft changes
do not invalidate accepted statistics. Opening the dialog starts calculation;
closing cancels its job, including a late admission. `App` retains modal/video
occlusion and passes navigation into the existing protected editor flow. There is
no statistics acceptance, save command or second document owner.

IPC pages cap at 200 findings and the UI displays 100. Existing 100,000 cue/cut/
finding and 16 MiB text ceilings apply. One-based exclusions also cap at 100,000;
summary size includes the canonical selection. These ceilings do not promise
latency for every maximum-size document.

## Validation

Ten core tests, two CLI parity/error tests and four desktop-owner tests pass.
Thirteen modeled assertions per Chromium/WebKit cover draft preservation,
protected navigation and failed Apply, scope/exclusion failures, paging, empty
reports, stale/late replies, cancellation, failure retry and disposal.

The actual Mac app matches executable CLI JSON for whole-document, range/excluded
and all-excluded scopes. It preserves source bytes, accepted cues, history and
drafts, exercises navigation Cancel/Discard/Apply with exact Undo, rejects invalid
scope and stale profile/document/media/scenes, and rescans real generated media.
The normal Settings/text/save/recovery/restart checks also pass. The inspected
summary, scoped and paged-outlier screenshots remain with the run receipt.

Predeclared 10,000-cue/940,000-byte/10,000-cut budgets: release full calculation
p95 <500 ms over 20 warm runs, <96 MiB additional sampled RSS, and desktop input,
cancellation, invalidation and navigation <1 second. Whole-document and a range
with exclusions both use full-document checking. The first RSS run fails at
114,638,848 added bytes. Bounded reservation of the scoped finding vector fixes
its repeated growth; v2 passes with 11.003/9.675 ms p95 and 21,676,032 added bytes.

The actual debug WebView reports 10,000 cues in 449 ms, with 4 ms generated draft
interaction, 1 ms cancellation, 12 ms invalidation and 125 ms outlier navigation.
Both declared workloads have exact expected populations and bounded finding
pages (200 IPC, 100 UI). Desktop events are scripted; native file choices are
supplied by the isolated harness. These observations do not establish physical
input, GPU/audio presentation or large-video detection accuracy.

The first browser run found missing reactive capture tracking, now corrected.
Subsequent harness fixes normalize source whitespace and select the first of
several valid links to the same cue. An early desktop attempt was stopped before
connecting because its instrumented build had not yet completed. All failures
and the final passing run are retained; none relaxed acceptance budgets.

```sh
cargo test --locked -p vsf-application statistics
cargo test --locked -p vsf-cli --test statistics
cargo test --locked -p visualsubforge document::statistics
npm run test:browser -- --grep 'workflow: statistics'
cargo build --locked --release -p vsf-application --example statistics-benchmark
python3 scripts/statistics-benchmark.py
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked -p vsf-cli
npm run test:desktop -- --statistics
```

Final regression commands: `cargo test --locked --workspace` (271 ordinary tests),
`cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo fmt --all
-- --check`, `npm run check` (zero errors/warnings), `npm run lint`, `npm test`
(39), `npm run test:browser` (46 Chromium/WebKit cases), `npm run format:check`,
`npm run ipc:check`, and `npm run tauri -- build --debug --no-bundle -- --locked`.
The explicit IPC exporter is separate from the ordinary Rust count; zero-test
and ignored media harnesses are not feature evidence.

Evidence is retained under `tests/fixtures/generated/statistics/`, including
`desktop-final/`, `benchmark-v1.json`, `benchmark-v2.json` and command logs. RSS samples
cover the benchmark process, not total WebView/native child memory. No new
Windows/Linux feature/installation or physical GPU/audio acceptance is claimed.
Full ASS/SSA document/style editing remains separate.
