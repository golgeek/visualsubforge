# Desktop and browser workflow tests

Run commands from the repository root using the [pinned toolchains and Tauri
prerequisites](../../CONTRIBUTING.md#commands-and-toolchains). This guide describes
how to reproduce checks and interpret their scope. Record exact commands, source
and runtime identities, results, failures and unavailable checks in the PR
description. Keep dated receipts and task histories outside public documentation.

## Default desktop suite

```sh
npm ci
npm run build:desktop-test
npm run test:desktop
```

On headless Linux, install `xvfb`, `xauth` and `dbus`, then run:

```sh
GDK_BACKEND=x11 xvfb-run -a -s '-screen 0 1280x800x24 -nolisten tcp' dbus-run-session -- npm run test:desktop
```

The build enables `desktop-e2e` and uses [tauri.test.json](tauri.test.json).
It builds the editor and its sibling `vsf` CLI. The runner defaults to
`target/debug/visualsubforge` and `target/debug/vsf` (with `.exe` on Windows).
Override them with `--binary /absolute/path/to/editor` and
`--cli /absolute/path/to/vsf`. Rebuild normally before using the debug executable
as an ordinary editor; never distribute the instrumented app.

The suite runs production components in Tauri's actual WebView and sends real
Rust commands. It covers:

- Unicode/BOM/CRLF SRT editing, stable cue IDs, complete Undo/Redo restoration,
  cancelled/failed Save As, exact save/reopen and external-writer protection.
- Durable accepted-state recovery after terminating an owned process, a separate
  recovery Save As and normal shutdown. This does not prove power-loss durability.
- Quality Settings import/export, failed persistence, stale-dialog rejection,
  restart restoration, accepted/draft metrics, protected Problems navigation,
  usable list/editor layout and executable CLI parity.
- Read-only minimum-gap proposals, no-media acknowledgment, atomic acceptance
  and Undo/Redo; tag-aware text search/replacement, join/reflow skips and formatting
  that remains a draft until Apply.
- Command palette and searchable shortcut sheet through the actual WebView,
  generated key input, focus restoration and palette bounds at requested
  1100×720 / 1440×900 window sizes.
- Refresh-only stale-edit reconciliation that retains the draft without replaying
  the mutation or adding history.

## The test boundary

The pinned embedded WebDriver serves a loopback port for the owned application;
no external driver or native media SDK is needed for the default suite. Normal
builds exclude the driver. The harness supplies queued file selections and an
isolated app-data directory, not document replies or a second accepted-state
owner. `VSF_E2E_ROOT` confines fixture paths; it is not an OS sandbox.

The runner waits for the application document, Tauri APIs, editor and completed
workspace initialization. Waits are bounded. It terminates only processes it
spawned and retains failure evidence. A startup timeout is not a completed
workflow. CI's default runtime step has its own five-minute deadline.

Each invocation creates an ignored `test-results/desktop-*` directory containing
inputs, isolated recovery/settings data, process logs, `receipt.json`, screenshots
and failure DOM where available. Optional lanes add their own JSON reports.
These are generated outputs, not files supplied by a fresh clone. Attach relevant
receipts to the PR or link its CI artifacts, with their retention limits; do not
use a local output path as public proof that a check passed.

Generated DOM events exercise components and IPC, not physical input delivery.
The default suite does not cover native menu clicks, OS file dialogs, media
playback, GPU/audio output or installation. WebView screenshots can omit the
native video sibling. Compilation, a zero-test harness, a preview acknowledgment
and a state-only audio report each prove less than an observed native frame or
verified audio content.

## Native-feature build

Optional media lanes require the [Mac build recipe](../../docs/build-macos.md) or
the experimental [Windows/Linux native recipes](../../docs/native-runtime.md),
including an explicitly selected SDK/runtime and FFmpeg/FFprobe. After configuring
the native dependencies:

```sh
npm run tauri -- build --debug --no-bundle --features desktop-e2e,media-prototype --config tests/desktop/tauri.test.json -- --locked
cargo build --locked -p vsf-cli
npm run test:desktop -- --duration
```

For a Mac private dylib selected through `DYLD_LIBRARY_PATH`, invoke the pinned
Node executable directly; npm's shell can remove that environment variable:

```sh
DYLD_LIBRARY_PATH=/absolute/runtime/directory /absolute/pinned/node tests/desktop/run.mjs --duration --binary /absolute/path/to/instrumented/editor --cli /absolute/path/to/matching/vsf
```

Keep the intended app foreground when observing native input/focus. Identify any
foreground driver or generated-input conditioning in the PR; it does not establish
physical input, speaker output, native composition or other-platform acceptance.

## Optional feature suites

Append flags to `npm run test:desktop --`, or to the direct Node command above.
The ordinary persistence/quality/text/recovery workflow runs before these lanes.
Use the native-feature build for the media workflows in this table; the explicitly
media-free plugin editor mode uses the default build. Generate the resync fixture
with `python3 scripts/resync-fixtures.py` before lanes that consume it.

| Flag | Coverage and additional prerequisites |
| --- | --- |
| `--scenes` | Generated known-cut media/caches, actual FFmpeg rescanning, protected drafts, mapped coverage, timed-tag refusal, exact save/Undo and stale media/scene rejection. Includes a 10k-cue/cut paging, acceptance, cancellation and invalidation case. |
| `--duration` | Selected/all duration proposals, fixed starts and static tags, timed-tag skips, exact save/history and rescan rejection; includes its own 10k-cue/cut fixture. Can combine with `--scenes`. See [duration policy](../../docs/duration-repairs.md). |
| `--statistics` | Desktop/CLI parity, scope/exclusions, unavailable populations, protected navigation, unchanged history/source, profile/media/rescan invalidation and 10k paging/cancellation. Can combine with scenes/duration. |
| `--ass` | ASS/SSA precision, styles, text/time edits, exact saves, project/recovery, tagged-SRT timing and native preview acknowledgments. The font case requires the explicitly named installed Mac font. See [format scope](../../docs/ass-ssa-documents.md). |
| `--batch` | Paused-playhead/disjoint alignment, tagged SRT, exact saves and Undo/Redo; also runs the ASS performance/batch gate. Generate ASS fixtures first. |
| `--resync` | ASS/SSA/tagged-SRT CLI parity, timed-effect refusal, protected save/history and preview acknowledgment. Generate resync fixtures first. |
| `--language` | English/French reviewed edits, personal-word persistence and failure handling, history, project/recovery and protected source. Generate resync fixtures first. See [language tools](../../docs/language-tools.md). |
| `--styles` | ASS/SSA create/copy/edit/rename, exact references, history, save failure, project and source-property recovery. Generate resync fixtures first. |
| `--fonts` | ASS/SSA attachment inventory/add/replace/remove, invalid import, picker cancellation, exact saves/history/project/recovery and preview revisions. Generate resync fixtures first. |
| `--conversion` | Six SRT/ASS/SSA export directions, loss review, cancelled/failed destinations, unchanged source/history/project, reopen/recovery and timed rounding. Generate resync fixtures first. |
| `--webvtt` | Millisecond edits, formatting drafts, protected timestamps, CSS/region preservation, exact saves, CLI metrics, project/recovery and external conflicts. See [preview limits](../../docs/webvtt.md). |
| `--legacy-import` | Reviewed SRT/ASS/SSA/WebVTT and WAV-only legacy project publication/restoration, relocation, stale inputs, protected opening, exact save/recovery and unchanged active state. See [supported subset](../../docs/legacy-project-import.md). |
| `--translation` | Manual reference/target copies across formats, offsets, missing/changed references, source protection, exact history, project/recovery and target preview. See [manual translation](../../docs/manual-translation.md). |
| `--plugins` | Modern/VSS checker/fix cases across formats, cancellation/reaping, package changes, exceptions, exact state/history/save/recovery and native preview. Needs the sibling CLI and FFmpeg. |
| `--plugins-editor` | Same plugin editor cases without media/FFmpeg/native preview. Choose this or `--plugins`, not both. |

These synthetic inputs and supplied choices do not prove real chooser behavior,
large-video detection accuracy, physical input or GPU/audio acceptance. Contracts
for [styles/fonts](../../docs/style-authoring.md), [conversion](../../docs/format-conversion.md)
and [plugins](../../docs/plugin-desktop.md) define additional limits.

## Large-document suites

Run performance lanes separately to avoid competing benchmark CPU work. The
runner verifies fixture hashes where it consumes the shared corpus. Generate it
before the relevant lanes:

```sh
python3 scripts/ass-fixtures.py
python3 scripts/webvtt-fixtures.py
npm run test:desktop -- --ass-performance
```

| Flag | Workload and scope |
| --- | --- |
| `--performance` | Generated 10k SRT: quality checks, report/DOM bounds, typing, pause, cancellation, revision guards, gap review and text proposals with full-batch acceptance and Undo. |
| `--ass-performance` | Exact ASS/SSA 10k inputs: open, text/time/style edits, Undo, navigation, typing, cancellation, source preservation and bounded pages. Uses the default test build without media. |
| `--style-performance` | Exact ASS/SSA 10k form/accept/Undo, cancellation and stale guards. |
| `--font-performance` | Five add/replace/remove cycles for both 10k formats, inventory, Undo, cancellation and stale guards. |
| `--language-performance` | Six generated language/format 10k workloads, reviewed editing and cancellation; whole worker duration is separate from interaction latency. |
| `--conversion-performance` | Six 10k export workloads, five review/export cycles, exact repeated output, bounded review, cancellation and reopen. |
| `--webvtt-performance` | Plain/styled/timed 10k WebVTT: five edit/history/save cycles, full reopen/recovery, virtual navigation and stale/cancel guards. |
| `--legacy-import-performance` | Exact WebVTT and ASS 10k inputs, repeated review/publication, full project restoration/history/save/recovery and stale/cancel guards. Loads generated video. |
| `--translation-performance` | Five exact reference/target workloads, all reference pages, repeated offset/edit/history/save/project cycles and captured-reference recovery. Loads no media. |
| `--plugin-performance` | Five exact inputs with both plugin APIs, all finding/change pages, repeated check/fix/accept/history/save cycles, cancellation, source protection and recovery. Loads no media; requires sibling CLI. |

Use the feature's native-build recipe for authoring/translation/plugin benchmark
bundles, even when a particular workload loads no media. Most interaction gates
are one second; background preparation is recorded separately and retains its
own bounded deadline. Reports record individual measurements, identities and
scope. App-process RSS observations exclude WebView subprocesses; debug WebView/IPC
measurements do not replace release engine benchmarks or physical presentation
latency measurements. Diagnostic subsets and instrumented profiles cannot replace
a complete uninstrumented acceptance run.

## Independent native observations

Where supported, a capture flag writes `capture-request.json` and waits up to
60 seconds for `capture-ack.json` with the same `label`. Inspect and save an actual
native-window image before acknowledging it. Never acknowledge from WebView
screenshots or backend readiness alone.

| Lane | Capture flag | Optional label filter |
| --- | --- | --- |
| ASS | `VSF_ASS_DESKTOP_CAPTURE=1` | None |
| Fonts | `VSF_FONT_DESKTOP_CAPTURE=1` | `VSF_FONT_DESKTOP_CAPTURE_LABELS` |
| Conversion | `VSF_CONVERSION_CAPTURE=1` | `VSF_CONVERSION_CAPTURE_LABELS` |
| WebVTT | `VSF_WEBVTT_CAPTURE=1` | `VSF_WEBVTT_CAPTURE_LABELS` |
| Legacy import | `VSF_LEGACY_IMPORT_CAPTURE=1` | `VSF_LEGACY_IMPORT_CAPTURE_LABELS` |
| Translation | `VSF_TRANSLATION_CAPTURE=1` | `VSF_TRANSLATION_CAPTURE_LABELS` |
| Plugins | `VSF_PLUGIN_CAPTURE=1` | `VSF_PLUGIN_CAPTURE_LABELS` |

Filters are comma-separated labels. Record actual runtime identity, display scale,
observation method and uncovered cases in the PR description.

## Native audio re-entry and replacement (macOS and Linux)

`--native-audio` uses the native-feature bundle and selected private libmpv. It
generates two stereo WAVs and checks looping, seeking past B through EOF,
re-entry, before-A seeks, playing-media replacement, old-session refusal and
unchanged subtitles/source. The before-A lead-in and Linux loop-clear clock check
are status observations, not complete PCM captures of those boundaries.

On macOS 14.2 or newer, build the process-only capture helper:

```sh
mkdir -p tests/fixtures/generated
clang++ -std=c++17 -fobjc-arc -mmacosx-version-min=14.2 -Wall -Wextra -Werror \
  tests/native/process-audio-capture.mm -framework Foundation -framework CoreAudio \
  -Wl,-sectcreate,__TEXT,__info_plist,tests/native/audio-capture-info.plist \
  -o tests/fixtures/generated/process-audio-capture

VSF_PROCESS_AUDIO_CAPTURE="$PWD/tests/fixtures/generated/process-audio-capture" \
VSF_AUDIO_ANALYSIS_PYTHON=/absolute/path/to/python-with-numpy \
VSF_EXPECT_MPV=/absolute/path/to/selected/libmpv.2.dylib \
DYLD_LIBRARY_PATH=/absolute/path/to/selected/library-directory \
/absolute/pinned/node tests/desktop/run.mjs --native-audio \
  --binary /absolute/path/to/native/editor --cli /absolute/path/to/native/vsf
```

Install `tests/native/requirements.txt` in the selected Python environment. The
Mac helper may require audio-capture consent; it records only the owned editor PID
through a private tap, not a microphone or a whole system device. On Linux use
`tests/native/pulse-stream-capture.py`, `pactl`, `parec` and a server supporting
single-stream monitoring. Exactly one sink input must match the owned editor PID;
a whole-sink recording is not a substitute. Record the selected runtime identity.

For a VM without NumPy, add `--native-audio-capture-only` alongside `--native-audio`,
then analyze the unchanged files on a host with NumPy:

```sh
/absolute/path/to/python-with-numpy tests/native/audio-editor-content.py \
  --report /host/shared/evidence/native-audio.json \
  --output /host/shared/evidence/native-audio-content.json \
  --path-map /guest/shared /host/shared
```

The analyzer verifies hashes, process identity, format, complete PCM blocks and
both channels. State/capture completion is not a content pass; an incomplete
workflow cannot pass content analysis. Linux receive times are not presentation
timestamps. This does not establish physical pointer, real-sheet, DAC/speaker,
changing-rate or other-platform acceptance. See [native reliability limits](../../docs/native-reliability.md).

## Subtitle plugins

Use `--plugins` or `--plugins-editor` from the feature table, and
`--plugin-performance` for the complete scale lane. To test process observation
independently with a real sibling CLI helper:

```sh
node --test tests/desktop/plugin-processes.test.mjs
```

This checks live observation and a fresh empty snapshot after EOF, not live-loop
UI cancellation. The production plugin owner must cancel and reap its helper;
the observer must not signal it.

## ASS/SSA documents and tagged SRT

Use `--ass` for the native feature workflow and `--ass-performance` for the
media-free scale lane described above. ASS/SSA precision is 10 ms; tagged SRT
retains millisecond precision. Native preview acknowledgments and WebView PNGs
do not prove native composition. The ASS capture flag supports separate observation.

## Browser workflow checks

```sh
npm ci
npx playwright install chromium webkit
npm run test:browser
```

On Linux, use `npx playwright install --with-deps chromium webkit` for system
prerequisites. The runner starts its own Vite server on port 1421 (override with
`VSF_BROWSER_TEST_PORT`) and refuses to reuse an existing server. It runs the
component and workspace lifecycle pages in both engines, including project save
failures, recovery ordering, focus and gesture cancellation. FAIL rows, page errors,
incomplete/empty results and inconsistent counts fail the run; negative controls
exercise those failure modes.

Reports, failure screenshots and traces are generated under ignored
`playwright-report/` and `test-results/browser/`; CI uploads them. Browser cleanup
preserves desktop receipts. These tests use production components with modeled
IPC and synthetic events; they do not establish native WebView, filesystem,
physical keyboard, GPU or audio-device behavior.

The canonical desktop run requests 1100×720 and 1440×900 windows, switches all
five workspace presets, and records ten screenshots plus requested/actual sizes.
Some hosted displays clamp window height. The receipt lists unavailable sizes;
a clamped run does not establish acceptance at the requested larger size. Timing
requires eight rows in the smaller tested client area and fourteen when the host
supplies the requested 900-pixel window. Exact-size browser cases cover the smaller
and larger client budgets independently. Every preset checks two editor text
lines and video-placeholder separation. Playback-free runs verify DOM bounds;
the browser layout harness checks modeled hide/bounds acknowledgments. Neither
establishes physical native-video composition acceptance.

The cue-columns browser harness records 120 animation-frame intervals while
scrolling 10,000 generated cues, plus the maximum mounted-row count and browser
identity. Interpret mean/p95/max as an observation of that test environment, not
a portable performance threshold or native-video/real-waveform benchmark. Column
fit measures only mounted rows; both fixed row densities retain virtualization.


Cue-editor fixtures use `editor-input.mjs` for generated Ctrl+Enter/Escape through
the production component handlers. The removed Apply/Revert buttons are not
emulated, and these helpers never invoke document mutations directly. Individual
workflows still wait for accepted snapshots, exact fields or inline rejection.
Pause/blur/IME triggering has separate browser coverage; generated keyboard events
do not establish physical native-input acceptance.
