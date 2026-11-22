# Duration and reading-speed repairs

The **Fix reading speed…** command opens the duration and reading speed preset. The shared guide describes the workflow, exact rate bounds, nearest safe endpoints, counts and manual text budgets. [Fix all timing](timing-repair-all.md)

## Historical validation

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

## Historical status receipt: Duration/speed milestone complete (2026-10-17)

Relocated from the status page. This receipt retains the scope and pending
work at that checkpoint; subsequent evidence and the [current status](status.md)
supersede its staging statements. Moving it adds no validation claim.

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

The unified engine's independent selection oracle uses seed `0x035f2026`.
Run `VSF_TIMING_ORACLE_CASES=20000 cargo test --release --locked -p vsf-application --lib independent_selection_oracle -- --nocapture` to exercise larger samples.
Set `FUZZ_DEPS=1` to measure dependency and conflict precision and the number
of other rows lost on an untick. The acceptance metric verifies every claimed
fix against the final report. The test also prints the safety-only counts for
comparison. Set `VSF_TIMING_ORACLE_MAX_CUES` to vary the document size.
