# Real desktop workflow test

This lane runs the Svelte editor in Tauri's actual webview, using the generated
client and real Rust document, persistence and recovery operations. It complements
the controlled-response browser tests in `tests/frontend`; it does not replace them.

From the repository root with the pinned Node/Rust toolchains and ordinary Tauri
development prerequisites:

```sh
npm ci
npm run build:desktop-test
npm run test:desktop
```

On headless Linux, install `xvfb`, `xauth` and `dbus`, then run:

```sh
GDK_BACKEND=x11 xvfb-run -a -s '-screen 0 1280x800x24 -nolisten tcp' dbus-run-session -- npm run test:desktop
```

`npm run test:desktop -- --binary /absolute/path/to/visualsubforge` selects another
instrumented executable. The default is `target/debug/visualsubforge` (`.exe` on
Windows). The test build replaces that debug executable; rebuild normally before
using it as an ordinary editor.
It also builds `vsf-cli` for the executable parity check. `--cli /absolute/path/to/vsf`
selects a matching CLI; by default the runner uses `target/debug/vsf` (`.exe` on Windows).

## What it verifies

The runner opens a BOM/CRLF Unicode SRT, commits a text edit, checks stable cue
identities and complete Undo/Redo restoration, and exercises cancelled and failed
Save As before saving and reopening an exact-byte copy. It waits for a real
recovery checkpoint, abruptly terminates its own app, modifies the saved file to
simulate an external editor, then relaunches and uses the recovery UI. It verifies
that the external edit survives a rejected save, saves recovery elsewhere and
requests normal application shutdown. It also exercises quality profile selection,
customization, cancelled/invalid imports, export/reimport and timing-only imports
through the real Settings UI. An actual failed settings-file replacement preserves
accepted rules; retry persists them, a stale dialog is rejected, and a second
process restores the exact saved profile. All these operations must leave the
loaded subtitle document and Undo history unchanged.

The quality workflow then enables accepted/draft metrics and Problems without
media, verifies protected Cancel/Discard navigation, and checks that the list
and editor retain usable space. It exports Rust's accepted profile, executes
`vsf check` on the same SRT, and compares the profile/hash, every metric/finding
and affected-cue count. The partial profile must remain explicitly incomplete.
Screenshots cover both the panel controls and scrolled findings; the JSON parity
receipt preserves both actual outputs. Neither checking nor draft metrics may
change the accepted document, history or save eligibility.

Minimum-gap review then verifies protected draft cancellation/discard, a read-only
preview, explicit no-media acknowledgment and atomic acceptance. One Undo restores
every cue and the original dirty/history state; Redo reapplies the plan. The normal
save/recovery checks run afterward. The explicit history buttons must use document
history even when a text field retained focus during the click; menu/keyboard Undo
still uses the focused field's local history. Modal closure waits for native
occlusion/focus restoration before further workspace actions.

Text tools then exercise draft cancellation/discard, Unicode source-match
navigation, single replacement and Replace All through real IPC. The lane checks
exact ASS/HTML preservation, stable IDs/times, one-step Undo/Redo, failed-save
preservation, join/reflow acceptance with positioned-text skips, and formatting
that remains a draft until Apply. The temporary fixture edits are undone before
the existing save/recovery workflow continues.

Each run retains `test-results/desktop-*`: its inputs/outputs, isolated recovery
data, per-process logs, receipt and screenshot (plus DOM on failure). The runner
has bounded waits and terminates only processes it spawned. CI additionally caps
the run step at five minutes. Recovery after process termination does not prove
power-loss durability.

Startup waits for the actual application's document title before the first
script evaluation, then separately requires Tauri APIs, the editor and completed
workspace initialization. The embedded driver's window-ready status can precede
initial navigation; its multi-evaluation script result can be lost across that
navigation. Document title/URL evidence is retained for each launch. This avoids
starting a script in the initial blank document without retries or longer script
deadlines. The real Mac workflow passes locally with this readiness check;
the recurring hosted second-launch timeout still needs replacement-run evidence.

## The test boundary

The explicit `desktop-e2e` Cargo feature enables pinned
`tauri-plugin-wdio-webdriver` 1.5.0; the Node runner uses pinned WebdriverIO 10.0.2.
The embedded driver serves standard WebDriver on a loopback port chosen for the
run. No external driver, IPC-mocking plugin or native media SDK is required.
Normal builds exclude the driver and the test module.

`tauri.test.json` gives the test app a separate identifier and permits test code
to emit the existing workspace-action event. The runner emits those menu actions
and uses ordinary webview controls; document writes still travel through the real
frontend client. Read-only status probes call the real Rust command.

Two services are deliberately supplied by the harness: a queue of file selections
(including cancellation) and a unique app-data directory. `desktop_test.rs`
requires the absolute `VSF_E2E_ROOT`, confines queued paths to it, and redirects
settings/recovery there. These are fixture constraints, not an OS sandbox.
There are no mock document replies, test-only document commands or alternate
accepted-state owners. The optional driver lives for the app process; the runner
owns process cleanup and retains evidence even on failure.

The default lane does **not** exercise native menu clicks, native file dialogs,
media playback, GPU/audio output or the resync proposal walkthrough. Those have separate
backend/browser tests and native acceptance checks. Do not distribute the
instrumented executable as a production app.

Local evidence on 2026-10-11: the workflow passes on macOS ARM64, Ubuntu 26.04
ARM64 under Xvfb, and Windows 11 ARM64. Hosted default-app execution also passes
on macOS 14, Windows Server 2022 and Ubuntu 24.04 at `ce43559`
([run](https://github.com/golgeek/visualsubforge/actions/runs/38167423845)); that
run's separate modeled browser job failed and was corrected subsequently.
The first Mac/Linux runs
incorrectly expected ordinary recovery to prohibit any reuse of its unchanged
source; the corrected scenario tests the actual external-edit conflict guarantee.
Resync's stricter distinct-file rule is covered separately.

The expanded quality Settings/checker/CLI-parity/restart workflow passes on macOS ARM64 on
2026-10-17; this addition has not yet been executed on Windows/Linux. The pinned
embedded driver uses JavaScript clicks, so select options require explicit DOM
input/change events in this harness. This verifies the real component/IPC path,
not physical keyboard or pointer delivery. Retained current evidence is copied
under `tests/fixtures/generated/quality/checks-desktop-final/`; the expanded gap
workflow is under `quality/gaps-desktop-final/` (2026-10-17, macOS ARM64).

## Optional large-document measurement

Run `npm run test:desktop -- --performance` after the same instrumented build.
It performs the regular workflow and then measures a generated 10,000-cue SRT
with 1,050,000 text bytes. Twenty warm real check round trips, bounded report
pages/DOM, typing during admitted checking, visible pause, worker cancellation,
stale-revision rejection and UI reconciliation are recorded. The interaction
budget was declared as one second before measurement. It checks Undo restoration
and a 9,999-candidate read-only gap preview with 100 table rows. A recovery banner
and metrics must not overlap or clip the history controls.

The same 10,000-cue document then exercises text proposals, typing during work,
cancellation, 200-cue IPC pages, 100 visible review rows, accepting the complete
batch from a later page, one Undo and revision invalidation. `text-performance.json`
records these separately, with a one-second interaction/cancellation/invalidation
budget. Full proposal duration is reported separately from the release engine
benchmark in the [text tools guide](../../docs/text-editing.md).

`quality-performance.json` retains binary/input hashes, profile, raw timings,
main-process RSS samples and rAF intervals. RSS uses macOS/Linux `ps`; the final
measurement was on macOS ARM64. These are generated webview events and actual IPC,
not physical input, GPU presentation or total multiprocess memory. Desktop debug
round trips and RSS observations are separate from the release pure-engine
benchmark. Final evidence: `quality/performance-layout-final/`. See the
[quality guide](../../docs/subtitle-quality.md) for results and exact limits.

The first Windows run reached the save-failure check, then failed because Rust's
canonical path had the Windows extended-length prefix. The runner now normalizes
that representation with Node's `toNamespacedPath`, retaining a complete path
comparison. No product save behavior changed.

Dependency review replaced the initial WebdriverIO 9.31.9 trial with 10.0.2,
removing the affected archive/proxy dependency chain reported by `npm audit`.
The runner follows v10's documented async `execute` API; it still uses the same
standard WebDriver endpoint on the embedded plugin. The existing `source-map-js`
1.2.1 lock entry was also updated to the compatible 1.2.2 security patch.
`npm audit --json` reports zero advisories on 2026-10-11; this is a dated registry
check, not proof of vulnerability-free dependencies. Upgrade these pins with the
same real-app checks on each OS. See the
[v10 changes](https://github.com/webdriverio/webdriverio/releases/tag/v10.0.0),
[archive advisory](https://github.com/advisories/GHSA-jmr9-qjv8-65gv) and
[source-map advisory](https://github.com/advisories/GHSA-68fv-2mgg-jv7q).

## Scene quality workflow and scale

Build the instrumented native-media app with the platform prerequisites; on the
development Mac the recorded run uses Homebrew mpv through the OpenGL adapter:

```sh
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked -p vsf-cli
npm run test:desktop -- --scenes
```

FFmpeg/FFprobe must resolve through the normal development tool policy. The
runner generates two tiny videos, SRTs and project caches with independently
known cut times and real file identities. It restores these caches through the
ordinary project path, then runs actual FFmpeg analysis on the small video.
The ordinary Settings/text/history/save/recovery workflow runs first.

The scene extension verifies Problems filtering and protected navigation,
read-only previews, static-tag preservation and a timed-tag skip, exact saved
bytes and unchanged source, failed saves, one Undo/Redo, rescan/media rejection,
and no-media coverage after detach. Its 10,000-cue section checks 10,000 findings,
100-row review pages, full acceptance from page two, exact Undo, typing,
cancellation and stale-revision invalidation against the predeclared one-second
interaction budget. It runs even without `--performance`.

The 2026-10-17 Mac run passes: preview 413 ms, acceptance 110 ms, generated draft
interaction 1 ms, cancellation 3 ms and invalidation 13 ms. It records
`scene-performance.json` and preview screenshots alongside the ordinary receipt.
The reproducible synthetic caches test project restoration and large-document
work; they do not measure large-video cut-detection accuracy. Webview events and
queued file choices do not establish physical input, native dialogs or GPU/audio
presentation. No new Windows/Linux execution is claimed. See
[scene quality](../../docs/scene-quality.md) for the retained evidence and limits.

## Duration and reading-speed workflow

Use the same `desktop-e2e,media-prototype` build and run
`npm run test:desktop -- --duration`, or combine it with scene regression using
`npm run test:desktop -- --scenes --duration`. `timing-fixtures.mjs` shares media
generation and bounded file identity between the two extensions. Both generate
known-cut caches and independently exercise actual FFmpeg rescanning.

Duration review verifies protected drafts, selected-cue and whole-document scope,
extensions/trims with fixed starts and exact static-tag preservation, a timed-tag
skip, failed-save preservation, exact BOM/CRLF save/reopen, one Undo/Redo, and stale
scene/media rejection. The scale fixture has 10,000 cues and cuts, 940,000 source
text bytes and alternating too-short/too-long durations. It checks every proposed
duration (880/1116 ms), 100-row paging, whole-batch acceptance from page two, exact
Undo, typing, cancellation and edit invalidation against the predeclared one-second
interaction budget.

The combined Mac run on 2026-10-17 passes: duration preview 418 ms, acceptance
113 ms, generated draft interaction 1 ms, cancellation 3 ms and invalidation
13 ms. Scene regression also passes (418 ms preview, 115 ms acceptance).
`duration-performance.json`, screenshots and the ordinary receipt are retained
in `tests/fixtures/generated/duration-quality/desktop-final/`. These are debug
webview/IPC measurements with Homebrew mpv/OpenGL, not release CPU benchmarks or
physical input/GPU/audio acceptance. See the
[duration contract](../../docs/duration-repairs.md) for the separate release
benchmark and conservative repair policy. Windows/Linux execution remains open.

## Document statistics workflow

Use the same `desktop-e2e,media-prototype` build and run
`npm run test:desktop -- --statistics`. It can also accompany `--scenes` and
`--duration`. The normal Settings/text/history/save/recovery workflow runs first.
Statistics compares actual desktop and CLI reports for whole-document, inclusive
range/exclusions and all-excluded scopes on a BOM/CRLF tagged Unicode fixture.
It verifies unchanged source/history, draft preservation, protected navigation
Cancel/Discard/Apply, invalid ranges, profile changes, media replacement and
actual FFmpeg rescanning. Only an explicit draft Apply may edit subtitles.

The 10,000-cue/10,000-cut fixture checks finite/unavailable populations, scoped
exclusions, 200-row IPC caps, 100-row UI pages, cancellation, typing, stale document
rejection, exact Undo and outlier navigation. The Mac run on 2026-10-17 passes:
449 ms report display, 4 ms generated draft interaction, 1 ms cancellation,
12 ms invalidation and 125 ms navigation, within the predeclared one-second
interaction budget. Screenshots, CLI parity, performance JSON and the ordinary
receipt are retained in `tests/fixtures/generated/statistics/desktop-final/`.
This uses Homebrew mpv/OpenGL and generated WebView events; it does not establish
physical input, GPU/audio, native-dialog or Windows/Linux acceptance. See the
[statistics contract](../../docs/statistics.md) for separate release measurements.

## ASS/SSA documents and tagged SRT

`--ass` adds the Mac native workflow for the synthetic `styles.ass` and
`styles.ssa` fixtures: inspect styles without mutation, reject an exact 1 ms edit,
commit style/text/time together, Undo/Redo, cancel/fail/save/reopen, restore a
project and recover after an abrupt owned-process restart. Exact saved bytes and
preservation of a newer external edit are asserted. It then edits/saves/reopens
mixed HTML/ASS SRT at 1.001/2.999 seconds and samples native motion, position,
color, hard breaks and expiry. A separate Hiragino Sans GB document checks an
explicitly installed Mac font; original font declarations are never substituted.

Build with `desktop-e2e,media-prototype`, the test config and an explicitly
selected native runtime; see the Mac build guide. For a private dylib selected
through DYLD_LIBRARY_PATH, launch the pinned Node executable directly: npm's
shell can remove that variable on macOS. For example, after configuring pkg-config:

```sh
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked -p vsf-cli
DYLD_LIBRARY_PATH=/absolute/runtime/directory /absolute/pinned/node tests/desktop/run.mjs --ass
```

The runner writes `ass-documents.json` and WebKit PNGs. WebKit screenshots can omit
the native media surface, so those PNGs alone do not establish composition.
`VSF_ASS_DESKTOP_CAPTURE=1` pauses each settled revision for an independent native
window observation: the observer reads `capture-request.json` and writes
`capture-ack.json` containing its matching `label` within 60 seconds. Observe the
actual rendered frame before acknowledging it. Backend readiness can precede
visible presentation. Record the loaded runtime with `record-native-runtime.py`.

`--ass-performance` adds the unchanged Task016 one-second desktop gate using
`scripts/ass-fixtures.py`'s exact 10,000-cue inputs. It works with the default test
build without media. It measures open (including filesystem time), text/time/style
edits and Undo, navigation, typing during work, cancellation and invalidation;
checks actual accepted values, complete restoration, source bytes and bounded
DOM/proposal pages; and records input/binary hashes in `ass-performance.json`.
Both flags can be combined. These synthetic inputs and generated DOM events do
not establish physical input latency, platform installations or GPU/audio acceptance.


## Batch timing

`--batch` runs the ordinary save/history/recovery regression, then real native
paused-playhead/disjoint alignment with tagged SRT, exact output and failed-save
protection. It also runs `--ass-performance` with the new batch gate: 10k Select
All, paged review, whole-batch acceptance/Undo/Redo, cancellation, typing and stale
acceptance rejection. Receipts retain the actual playhead (which can differ from
the requested video seek), binary/input hashes and the unchanged one-second
budget. See [batch policy and receipts](../../docs/batch-timing.md). The native
feature is required for the small playhead workflow; screenshots of WebKit do
not establish native video composition.

## Styled resynchronization lane

After generating `python3 scripts/resync-fixtures.py`, build the CLI and current
native desktop test application, then run `node tests/desktop/run.mjs --resync
--binary /absolute/path/to/visualsubforge`. The optional lane copies media inside
the isolated picker root, uses actual menu/dialog/IPC analysis and the real CLI,
and checks ASS/SSA/tagged-SRT output parity, timed-effect refusal, native preview
revision acknowledgment, protected saving and Undo/Redo. It retains per-format
review/refusal screenshots and SHA256 output digests. The ordinary persistence
and crash-recovery workflow runs first. Native frame composition and physical
GPU/audio acceptance are not established by these webview screenshots.


### Style authoring

`--styles` runs both ASS/SSA create/copy/edit/rename workflows with exact reference,
Undo/Redo, save, failed-write, project and source-property crash recovery checks.
`--style-performance` measures real 10k form/accept/Undo and owner cancellation/
stale rejection. They can run together with the native-enabled test binary and
explicit private runtime. See [the exact recipe, budgets and evidence](../../docs/style-authoring.md).
Generated DOM events and preview revision acknowledgments do not establish
physical input, GPU presentation or platform installation acceptance.


### Font attachments

`--fonts` exercises real ASS/SSA attachment inventory/add/replace/remove, exact
save/history/project/recovery, invalid imports, picker cancellation and native
preview revisions. `--font-performance` measures five cycles for each add/replace/
remove and both 10k formats, including inventory, acceptance, Undo, cancellation
and stale-edit guards. It records preparation separately from the unchanged
one-second interaction gate. `font-authoring.json` and `font-performance.json`
retain input/binary identity and scope. Use the native-feature bundle command in
style-authoring.md. Optional `VSF_FONT_DESKTOP_CAPTURE=1` uses the same matching
`capture-request.json`/`capture-ack.json` native-window observation lane as ASS,
with optional `VSF_FONT_DESKTOP_CAPTURE_LABELS`. Webview images omit sibling
native video. Physical platform acceptance remains separate.

### Deliberate conversion

`--conversion` exercises all six SRT/ASS/SSA export directions, including reviewed
losses, cancelled/failed destinations, unchanged source/history/project bytes,
reopen, ASS→SSA crash recovery, attachment-backed native preview and safe timed
rounding. It needs the native-feature bundle and generated resync video described
in [conversion evidence](../../docs/format-conversion.md). Editor geometry checks
cover wrapped controls and recovery notices.

`--conversion-performance` runs the same six accepted 10k workloads as the pure
benchmark, with five review/export cycles each, bounded report examples, exact
repeated output, cancel/stale rejection, Undo and reopen. Preparation retains its
ten-second worker deadline; interactive operations retain the one-second gate.
Input/binary/output hashes and raw timings are in `conversion-performance.json`.
Generate ASS/SSA inputs with `python3 scripts/ass-fixtures.py` first.

Both flags use supplied file choices and generated DOM events. They do not test
the OS file picker or physical input. For an independent native-video observation,
set `VSF_CONVERSION_CAPTURE=1` and optionally comma-separated
`VSF_CONVERSION_CAPTURE_LABELS`. The lane writes `capture-request.json` and waits
up to 60 seconds for `capture-ack.json` containing the same `label`; acknowledge
only after inspecting and saving the actual native window image. WebView images
alone omit the native video sibling. The JSON reports retain those scope limits.

### WebVTT

`--webvtt` uses the native-feature build to exercise actual open, millisecond
editing, formatting drafts, reviewed replacement, safe timing shifts and timed
marker refusal. It checks byte-exact BOM/CRLF save/reopen, cancelled/failed/wrong-
format destinations, Undo/Redo, project restoration, crash recovery and an
external-file conflict. Native and executable CLI caption metrics must agree.
The independently authored fixture retains CSS/region records, voice/language/
ruby/timestamp markup and literal ASS-looking text. `webvtt.json` records binary
and source hashes; the normal runner also requires final code0 shutdown.

Use the pinned private runtime and `desktop-e2e,media-prototype` build described
in [WebVTT evidence](../../docs/webvtt.md). File choices and DOM events are
supplied, so this lane does not establish native chooser or physical-input
acceptance. Set `VSF_WEBVTT_CAPTURE=1` and optionally comma-separated
`VSF_WEBVTT_CAPTURE_LABELS` to pause at settled native frames for up to 60 seconds.
The observer must save and inspect an actual native-window image before writing
`capture-ack.json` with the matching `label`; WebView-only images omit the video
sibling. Preview limitations and the separate desktop10k gate remain explicit.

Generate the exact library benchmark inputs with `python3 scripts/webvtt-fixtures.py`,
then add `--webvtt-performance` to the runner for the three 10,000-cue workloads.
It verifies their hashes before measuring five edit/Undo/Redo/save cycles, full
reopen and crash recovery, virtualized navigation, background cancellation and
stale-proposal rejection. Every measured interaction must stay below 1000 ms;
background preparation retains its ten-second deadline. `webvtt-performance.json`
records individual timings, source/output hashes and the executable identity.
No media is loaded during this lane; pure CPU/RSS measurements and actual native
preview/chooser observations are separate evidence in the WebVTT guide.

### Legacy project import

`--legacy-import` runs the ordinary persistence regression, then native import
review/publication/restoration for SRT/ASS/SSA/WebVTT and WAVFile-only media.
It checks relocation, Unicode/UTF-16 paths, unsupported notices, cancelled/failed/
competing destinations, stale inputs, unchanged active document/draft/history/
policy, protected opening, exact save/reopen and crash recovery conflicts.
`legacy-import.json` retains hashes, original project data and native status;
the runner requires normal code0 shutdown. See [the build recipe, retained failures
and scope](../../docs/legacy-project-import.md).

Use the native-feature test bundle and explicit private runtime. Supplied chooser
results and generated DOM events do not test OS sheets or physical input.
`VSF_LEGACY_IMPORT_CAPTURE=1` and optional comma-separated
`VSF_LEGACY_IMPORT_CAPTURE_LABELS` pause for an independent native observation.
Inspect/save the actual window before writing `capture-ack.json` with the matching
`label` within 60 seconds. WebView images alone omit sibling native video.
Audio load/paused-position status does not establish waveform or speaker output;
the current raw-WAV fixture exposes an unknown-start waveform limitation.

`--legacy-import-performance` uses the exact plain/styled/timed WebVTT and ASS
10,000-cue library fixtures, checking their hashes before five review/publication
cycles each. It measures full native project open/reopen, edit/Undo/Redo/save,
crash recovery and cancellation/stale-input rejection, and verifies original
bytes and complete restored cue arrays. The one-second interaction gate and
ten-second background deadline remain unchanged. Results, timings and executable
identity are retained in `legacy-import-performance.json`. Generate inputs with
`python3 scripts/webvtt-fixtures.py` and `python3 scripts/ass-fixtures.py` first.
Run this lane on its own to avoid competing benchmark CPU work. It loads a small
generated video for real project restoration, but does not measure physical
input latency, GPU presentation or speaker output.

### Manual translation

`--translation` runs the ordinary regression followed by SRT/ASS/SSA/WebVTT
independent target copies, protected/failed saves, offsets and draft preservation,
literal reference browsing, exact edits, Undo/Redo, project restoration and native
target preview. It covers explicit changed/missing-reference resolution, recovery
with deleted references and externally changed targets, subtitle-only projects and
mixed ASS-target/WebVTT-reference attachment. `translation.json` records source/
binary hashes and per-format results; the runner requires normal code0 shutdown.

Use the native-feature bundle and explicit private runtime in
[manual translation evidence](../../docs/manual-translation.md#native-desktop-workflows--2026-10-18).
The video fixture is generated using FFmpeg. Supplied chooser results and DOM
events do not establish OS-sheet or physical-input acceptance. Set
`VSF_TRANSLATION_CAPTURE=1` and optionally comma-separated
`VSF_TRANSLATION_CAPTURE_LABELS` to pause for independent native observation.
Save and inspect the actual native window before writing `capture-ack.json` with
the requested `label` within 60 seconds. WebView screenshots omit the native video
sibling. Real chooser and combined desktop10k acceptance remain separate.

`--translation-performance` extends the ordinary regression with five exact
10,000-cue reference/target workloads: plain, styled and timed WebVTT, ASS, and
dense overlapping tagged SRT. Generate the established fixtures with
`python3 scripts/webvtt-fixtures.py` and `python3 scripts/ass-fixtures.py` first.
The runner verifies source hashes, visits all 100 reference pages, and performs
five offset/edit/Undo/Redo/save/project cycles per workload. It reopens the full
project, checks cancellation/stale acceptance, and recovers captured reference
bytes after deleting the reference and externally replacing the target. Protected
source saves, detached-copy provenance, reattachment and exact outputs are checked.

Run against an isolated test bundle using the native recipe above:

```sh
source "$HOME/.nvm/nvm.sh"
nvm use --silent
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  node tests/desktop/run.mjs --translation-performance \
  --binary /absolute/path/to/IsolatedTest.app/Contents/MacOS/visualsubforge
```

`translation-performance.json` records the binary hash, input/output hashes,
every page and operation duration, checkpoint file sizes and app-process RSS
sampled every 50ms. Every measured interaction must remain below 1000ms; background
copy/attachment preparation has a separate 10s deadline. RSS excludes WebKit
subprocesses and is descriptive, not the separate pure-engine 96MiB budget.
The pure-engine 500ms p95/96MiB gates remain unchanged. This lane loads no media;
native playback and real OS-sheet acceptance use their separate workflows.

For diagnosis, `VSF_TRANSLATION_PERFORMANCE_WORKLOADS=dense` selects a subset and
`VSF_TRANSLATION_PROFILE_DRAW=1` records canvas draw counts/times. Instrumented or
partial runs do not replace the complete uninstrumented acceptance run.

### Subtitle plugins

`--plugins` uses the native-feature test bundle and installed sibling CLI to run
modern/VSS checker and fix workflows on SRT, ASS, SSA and WebVTT. It checks draft
and chooser cancellation, read-only findings, exact source/metadata preservation,
one-transaction Undo/Redo, cancelled/failed/exact saves, reopen and durable recovery
after owned-process termination. Controls observe a live looping helper, cancel
and verify reaping, reject package changes after review and preserve accepted
state after script exceptions. Each format also reaches an acknowledged native
preview frame. FFmpeg generates a short video. Process inspection uses Unix `ps`
or a prestarted Windows CIM observer; each post-cancel sample is fresh. The
observer never signals plugin helpers: the production job owner must reap them.

`--plugins-editor` runs the same eight format/API cases without loading media or
requiring FFmpeg/native playback. Use the ordinary `npm run build:desktop-test`
build with its sibling CLI. This mode writes `plugins-editor.json` with explicit
`withNativePreview: false` and no native-preview claims. Choose one plugin mode
per run. The ordinary quality/history/save/recovery prerequisite workflow still
runs, including its cue-list and text-editor layout assertions.

`node --test tests/desktop/plugin-processes.test.mjs` validates process observation
using an actual sibling CLI helper waiting for stdin, followed by a fresh empty
snapshot after EOF. It does not substitute for live-loop UI cancellation.

See the [native build commands and evidence](../../docs/plugin-desktop.md#native-workflows-and-real-chooser--2026-10-18).
`plugins.json` records app/CLI/input/output hashes, exact-state assertions and
native snapshots; the ordinary receipt requires normal code0 shutdown. Supplied
chooser results and DOM events do not test real OS sheets or physical input.
`VSF_PLUGIN_CAPTURE=1` with optional comma-separated `VSF_PLUGIN_CAPTURE_LABELS`
pauses selected captures for up to60s. Inspect and save the actual native window,
then write `capture-ack.json` with the requested `label`; WebView screenshots omit
the native video sibling. The separate normal-build chooser trial and remaining
desktop10k/historical-host/platform gates are recorded in the plugin guide.

`--plugin-performance` exercises the five exact plugin API/CLI10k inputs (dense
tagged SRT, ASS, SSA, styled and timed WebVTT), each with both APIs. Generate ASS/SSA
and WebVTT inputs using `scripts/ass-fixtures.py` and `scripts/webvtt-fixtures.py`.
The runner verifies hashes, all200 finding pages and all200 change pages, then
five complete check/fix/accept/Undo/Redo/save cycles per combination. Non-WebVTT
fixes combine a10ms shift with text edits; WebVTT fixes retain embedded absolute
timestamps. Exact bytes/cues/format properties, source protection, failed saves,
full reopen, cancellation/stale rejection and durable crash recovery are checked.

Run this lane by itself against the isolated bundle and sibling CLI above, with
the same explicit private runtime. `plugin-performance.json` records timings,
hashes and owned app/direct-helper RSS samples (Unix `ps`,50ms intervals). All
measured ordinary interactions must stay below1000ms. Complete asynchronous
check/fix preparation must stay below10000ms; start feedback remains below1000ms
and the helper's1000ms execution deadline is unchanged. RSS is descriptive and
excludes WebKit/native subprocesses; the separate500ms p95/192MiB API/CLI gates
are unchanged. This lane loads no media. `VSF_PLUGIN_PERFORMANCE_WORKLOADS=dense`
selects a diagnostic subset; it cannot replace the complete ten-combination run.
