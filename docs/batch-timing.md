# Cue selection and batch timing

Task 019 is complete within the available Mac environment. Rust owns accepted subtitles and
history; selection, the primary editing cue, range anchor and review controls are
transient frontend state.

## Selection

Click a cue to select it. Shift+click selects an inclusive range in file order;
Ctrl/Cmd+click toggles membership. Ctrl/Cmd+Shift+click adds a range to the existing
selection. In cue rows, arrows/Home/End move selection, Shift extends a range,
Ctrl/Cmd moves keyboard focus without changing selection, Ctrl/Cmd+Space toggles
the focused cue and Ctrl/Cmd+A selects all. Ranges work across virtualized rows.
The editor shows the primary cue; existing single-cue editing controls still
operate on that cue. Failed draft application leaves selection and text intact.

Surviving selected IDs remain selected after acceptance, Undo/Redo and edits.
Replacement documents start with their first cue selected. Deleted IDs are removed;
the existing structural-operation policy chooses the next primary editing cue.
Selection is not saved in subtitle files or recovery history.

## Shift / align cues

Choose **Shift / align cues…** above the cue list. Shifts accept signed integer
milliseconds: positive moves later, negative earlier. Scope choices are selected
cues, all cues, current cue and following **file rows**, or cues whose **start**
is at/after the captured playhead. File order and chronological order can differ.

Alignment moves the earliest selected start to a captured, paused and settled
playhead. It preserves every selected cue's relative offset. The playhead is
captured in milliseconds and rounded to the nearest source-format tick, with
ties later; review shows the resulting target and offset. SRT retains 1 ms and
ASS/SSA 10 ms precision. Entered shift offsets must already match the format tick.
The captured target stays fixed if playback later moves. Replacing media requires
a fresh proposal.

Review shows exact before/after endpoints in pages of 100 rows. Acceptance applies
all displayed pages as one history operation. Empty/unknown/duplicate selection,
zero change, negative time, overflow or unrepresentable endpoints reject the
whole proposal. Saving remains separate and existing failed-save/Save As guards
apply. No timing is silently clamped. Intentional new overlaps or gaps are allowed
and remain visible in Problems; these explicit shifts are not automatic repairs.

Uniform shifts leave duration, text, cue identities, order, styles, metadata and
attachments unchanged. Cue-relative karaoke, fades, motion and transforms move
with their cue; tag parameters and event-effect fields are preserved exactly.
Duration-changing resynchronization is a separate milestone with its own policy.

## Ownership walkthrough

`DocumentSession` owns only the accepted projection, immutable selection sets and
unapplied draft. `BatchTimingDialog` captures its inputs and uses the existing
`TimingRepairSession` cancellation/late-result guards. Request comparison uses
values, so recomputing a derived object does not invalidate an unchanged proposal.

Typed IPC translates the request on a blocking worker. The existing single timing
proposal owner captures the accepted document/format under its mutex and computes
outside locks with per-cue cancellation and a ten-second deadline. Batch, scene
and duration jobs have distinct purpose checks and share a bounded slot with no
queue. Media → scene → document guard order remains unchanged. Current document,
profile and media/scene context are rechecked before reads and acceptance; the
conservative scene/profile guards apply even though uniform shifts do not depend
on those settings. No additional worker or native resource is introduced.

The pure `vsf_application::batch_timing` planner creates an immutable proposal.
Acceptance uses the existing `Edit::Retime` whole-document transaction and source
serialization validation. Only its successful reply replaces the frontend
projection. Cancellation and shutdown join outside the document mutex.

## Validation evidence

Core and owner tests cover scope/precision/invalid arithmetic, relative effects,
one-step history, cancellation/stale/kind guards, exact SRT/ASS/SSA/embedded-font
save/reopen, failed Save As and accepted-state recovery. Browser selection and
review workflows pass in Chromium and WebKit. The actual Mac workflow and full regression suite pass; no Windows/Linux feature
execution is claimed yet.

Predeclared 10k budgets remain 500 ms release CPU, 96 MiB additional sampled RSS
and one second per desktop interaction. Run:

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p vsf-application --example batch-benchmark
python3 scripts/batch-benchmark.py
npm test
npm run test:browser
npm run lint
npm run format:check
npm run build
npm run ipc:check
node tests/desktop/run.mjs --batch --binary /absolute/path/to/instrumented/visualsubforge
```

The release example includes document capture, proposal, acceptance validation,
serialization and Undo on Task 016's unchanged 10k ASS/SSA fixtures. Initial
receipt `tests/fixtures/generated/batch-timing/benchmark-v1.json` passes: all/half
selection p95 46.586/34.881 ms for ASS and 41.238/30.107 ms for SSA; sampled RSS
increase 54,018,048 bytes. These measurements are application-library evidence.
Native compilation or IPC acknowledgment alone cannot establish visible video,
physical-input latency or platform acceptance.


Final checks pass 308 ordinary Rust tests, 44 frontend tests and all 50 Chromium/
WebKit cases. Strict workspace and native-feature Clippy, lint, formatting,
Svelte/type/build and generated IPC checks pass. This counts ordinary tests,
not zero-test harnesses or ignored diagnostics.

Actual Mac receipt: `test-results/desktop-jZBnT6`. The OpenGL instrumented binary
SHA256 is `4c3808ef8442465a3a1d347d32311a790080f867e1ddcb46e4b2deee607bee42`.
Build and launch used the same private `audio-integration-master-v1` runtime as
the font milestone, selected with process-local pkg-config and DYLD paths:

```sh
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype --config "$PWD/tests/fixtures/generated/fonts/tauri.native.json" -- --locked
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" "$HOME/.nvm/versions/node/v24.21.0/bin/node" tests/desktop/run.mjs --batch --binary target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" cargo clippy --locked -p media-player -p visualsubforge --all-targets --features desktop-e2e,media-prototype -- -D warnings
```

The native paused playhead settles at 5.042s after a requested 5.015s seek on the
24fps synthetic video. Review visibly aligns the earliest selected SRT cue from
1.001s to that captured 5.042s, moving two disjoint cues by exactly 4041ms. Native
preview revision acknowledgment, Undo/Redo, failed Save As and exact tagged-SRT
output pass; the original file remains unchanged. `batch-native-review.png` was
inspected for readable controls, target/offset and before/after rows. That WebKit
image and preview acknowledgment do not prove native video composition.

The 10k ASS/SSA desktop gate passes with unchanged source hashes and one-second
budget: Select All 21/16 ms, preview 317/316 ms, accept 161/156 ms, Undo 57/106 ms,
typing 1/1 ms, cancellation 6/5 ms and stale invalidation 73/61 ms. Review renders 100
rows, backend pages cap at 200, and the cue list retains 6 virtualized rows. The
existing text/style/time/navigation gates also pass. Normal shutdown exits 0.
These are generated DOM events through the real WebView and Rust IPC, not physical
input measurements, ANGLE acceptance or physical Windows/Linux GPU/audio checks.

Retained failures: browser-v1–v8 exposed harness waits and matching of the hidden
save-confirmation dialog; browser-v9 and the full suite pass after targeting the
open dialog and waiting for enabled controls. Native desktop-jQsJ0W required an
unrepresentable exact video seek; desktop-QYgX7B retained that requested time in
its expected output. Corrected tests use the actual settled/captured playhead;
no product rounding policy or budget was relaxed. Lint/Clippy failures were fixed
with immutable selection helpers and the idiomatic integer multiple check.
