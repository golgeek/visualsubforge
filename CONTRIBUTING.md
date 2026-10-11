# Development workflow

[Current status](docs/status.md) is the tracked authority for implementation,
validation and remaining gates. Local task/decision histories are optional context;
the commands here and native contract work without them.

## Working on a task

1. Define the focused outcome and acceptance criteria; update a local task if one exists.
2. Read the relevant code, expected behavior and available context.
3. Explain consequential design choices before implementing them; use a decision record for changes to architecture.
4. Implement one small, end-to-end behavior or one bounded feasibility experiment.
5. Validate it, record evidence, and update the relevant documentation.
6. Mark the task done only when its acceptance criteria are met. Record remaining work explicitly.

Use short-lived branches and focused commits. A review description should explain the resulting behavior, significant tradeoffs, and validation. Keep task IDs in descriptions when helpful.

## Definition of done

- Acceptance criteria met or explicitly revised with a reason.
- Relevant automated checks pass; unavailable platform checks are identified rather than implied.
- Errors, cancellation, and resource cleanup are handled where the change introduces them.
- Documentation and task status match the implementation.
- The change is understandable to someone learning Rust and Svelte.

For foundational features, add a short walkthrough tracing the UI action through application and domain code and back. Explain why ownership and state are arranged that way. Add walkthroughs when actual code exists.

## Validation strategy

- Domain tests cover timing invariants, editing operations, undo/redo, and parser/serializer behavior.
- Integration tests cover command boundaries, persistence failures, and media adapter lifecycle.
- UI tests cover important interactions, keyboard focus, and stale asynchronous responses.
- Actual desktop runs cover native playback, display scaling, rendering, and audio/video alignment on macOS, Windows, and Linux.

Compilation alone does not validate native playback. The media prototype must produce a platform evidence matrix.

## Commands and toolchains

For an end-to-end native build and launch on the current Apple Silicon Mac, use
[Build and run VSF](docs/build-macos.md). It connects the mpv/ANGLE builds to the
application and selects the intended runtime explicitly. Audio candidates remain
experimental until the [native adoption gates](docs/native-runtime.md#remaining-adoption-gates) pass.

Run all commands from the repository root. Rust 1.98.1 (including rustfmt and Clippy) is selected by `rust-toolchain.toml`. Node 24.21.0 and its bundled npm 11.19.0 are pinned by `.nvmrc` and `package.json`. Commit both `Cargo.lock` and `package-lock.json`; use locked commands below rather than updating dependencies during routine setup.

### Prerequisites

- Install [rustup](https://rust-lang.org/tools/install/).
- macOS: install Xcode Command Line Tools (`xcode-select --install`) or select a full Xcode installation.
- Windows: install Visual Studio Build Tools with **Desktop development with C++**, the Windows SDK, and WebView2. Use the MSVC Rust toolchain.
- Ubuntu 24.04: `sudo apt-get update` followed by `sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.
- Consult [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other distributions. These are development environments, not a supported release-platform promise.

### Install and launch

With [nvm](https://github.com/nvm-sh/nvm) loaded in your macOS/Linux shell:

```sh
nvm install
nvm use
node --version
npm --version
rustup show
npm ci
npm run tauri -- dev -- --locked
```

`nvm use` selects the version for the current shell; run it again in a new terminal. On Windows, install Node 24.21.0 with npm 11.19.0 using the official installer or a Windows-compatible version manager; POSIX nvm is not a Windows shell tool. CI reads `.nvmrc` directly through setup-node.

For native Windows ARM64 development, use the ARM64 Node distribution and Rust's `aarch64-pc-windows-msvc` host. Include **MSVC Build Tools for ARM64/ARM64EC** and a Windows SDK in the C++ build-tools installation; the default x64 compiler selection alone does not establish an ARM64 development environment. Verify `rustc -vV`, `node --version`, and `npm --version` before running the same locked checks below. The [status table](docs/status.md) distinguishes supplied-VM evidence from platform support.

Tauri starts Vite at `http://localhost:1420` and opens the desktop window. Port conflicts fail explicitly. Stop with Ctrl-C. `npm run dev` runs only the browser UI and is not evidence of desktop compatibility. No legacy checkout, FFmpeg, or libmpv installation is needed.

### Checks

```sh
# Pure Rust only: no Node, system webview packages, or native media needed.
cargo check --locked -p subtitle-core
cargo test --locked -p subtitle-core
cargo clippy --locked -p subtitle-core --all-targets -- -D warnings

# Frontend and formatting.
npm run format:check
npm run lint
npm test
npm run build
npm run ipc:check
cargo fmt --all -- --check

# Whole workspace; build frontend assets before compiling the desktop shell.
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
npm run tauri -- build --debug --no-bundle -- --locked
```

`npm run build` includes strict Svelte/TypeScript checking; `npm run check` runs that step alone. `npm run format` and `cargo fmt --all` apply formatting. Prettier covers frontend and configuration files; existing planning Markdown is excluded to preserve its formatting.

After changing a Rust IPC definition, run `npm run ipc:generate`, then `npm run
check` and the relevant browser workflows. `ipc:check` generates into a temporary
directory and fails on drift without writing repository files. Both commands
compile the desktop test harness and require its platform prerequisites and
frontend build assets. See the [IPC migration and contract checks](src/lib/README.md#generated-ipc).

The domain crate now has edit/history tests. `cargo test --locked -p subtitle-formats` checks SRT round trips and invalid-input handling; the desktop crate has persistence failure/reopen tests. Zero-test doc-test harnesses are not feature validation. The separate media-analysis experiment now has unit/lifecycle tests and opt-in FFmpeg integration tests. See [fixture generation and analysis checks](tests/fixtures/README.md).

### Browser and analysis regressions

Run the component workflow pages automatically with the pinned Playwright package:

```sh
npx playwright install chromium webkit
npm run test:browser
```

On Linux, use `npx playwright install --with-deps chromium webkit` to install
browser prerequisites too. The runner starts its own Vite server on port 1421
(override with `VSF_BROWSER_TEST_PORT`) and refuses to reuse an existing server.
It checks the component and workspace-lifecycle pages in `tests/frontend/` in Chromium and WebKit, including
project save failures, recovery response ordering and gesture cancellation.
FAIL rows, page errors, incomplete results, zero checks and inconsistent passing
counts fail the run. Negative controls verify those runner failure modes. Reports,
failure screenshots and traces are under ignored `playwright-report/` and
`test-results/`; CI uploads them. See the [Playwright CI guidance](https://playwright.dev/docs/ci).
These are production components with modeled IPC and synthetic events, not native
WebView, filesystem, keyboard, GPU or audio-device acceptance.

Both waveform and scene integrations need actual FFmpeg and synthetic fixtures:

```sh
python3 scripts/media-fixtures.py
python3 scripts/scene-fixtures.py
cargo test --locked -p media-analysis --test ffmpeg --test scenes -- --ignored --nocapture --test-threads=1
```

CI runs both fixture generators and all integrations; ordinary workspace tests
leave these integrations ignored. They validate decoding/analysis and cancellation,
not native playback or presentation.

The [resync experiment](tests/fixtures/resync/README.md) has a separate fixture
generator, three visual-signature integrations and a ground-truth evaluator. CI
runs the extended synthetic corpus, including three ten-minute content blocks.
The wider local reference-movie case meets its coverage target after sample
verification; a passing corpus is not general matching or product acceptance.

The [resync CLI](docs/resynchronization.md) builds with
`cargo build --locked -p vsf-cli`, without Node or desktop/native playback
dependencies. Run `cargo test --locked -p vsf-application -p vsf-cli` for policy,
persistence and executable checks. After generating the resync fixtures, also run:

```sh
cargo test --locked -p vsf-application --test resync_ffmpeg -- --ignored --nocapture --test-threads=1
cargo test --locked -p vsf-cli --test cli -- --ignored --nocapture --test-threads=1
```

The headless CI matrix configures these on macOS, Windows and Linux with recorded
package-manager FFmpeg versions; it is not evidence of hosted execution or native
device behavior. Unix executable cancellation tests send SIGINT and verify child
reaping. On Windows, build the CLI and run the console-event check separately:

```sh
cargo build --locked -p vsf-cli
python tests/cli/windows-signals.py --vsf target/debug/vsf.exe
```

It creates a private console, sends an actual Ctrl-C event, and checks exit 130,
child termination, unchanged inputs and absent outputs. A separate timeout case
checks exit 7. The controlled child ignores Ctrl-C, so the test cannot pass merely
because Windows delivered the event to both processes. The headless Windows CI
lane includes this check; local ARM64 execution is recorded in the resync guide.

### Native oracle controls

The Python analysis tools have synthetic positive/negative controls that require
no capture device or player. Use Python 3.14.7 (the CI pin) and an isolated environment:

```sh
python3 -m venv /tmp/vsf-native-controls
/tmp/vsf-native-controls/bin/python -m pip install --only-binary=:all: -r tests/native/requirements.txt
/tmp/vsf-native-controls/bin/python tests/native/check-controls.py
```

On Windows, choose a local virtual-environment directory and its `Scripts/python.exe`.
The runner executes every `tests/native/*-test.py` suite, with a per-suite timeout,
and fails for missing dependencies or any nonzero exit. CI runs it with the pinned
NumPy version. Passing synthetic controls establishes the oracle's behavior on
those inputs; it does not substitute for a new GPU/audio/process capture.

When changing native mpv client policy, also compile and run
`tests/native/mpv-client-test.cpp`. It supplies fake client calls to test queue
ordering/bounds, render/media error precedence and unavailable signed properties;
it does not validate GPU or audio-device behavior. With the Mac development
headers installed:

```sh
clang++ -std=c++17 -Wall -Wextra -Werror -DMPV_STATIC -I/opt/homebrew/include tests/native/mpv-client-test.cpp -o /tmp/vsf-mpv-client-test
/tmp/vsf-mpv-client-test
```

Linux uses the same C++17 source with mpv's pkg-config include flags. Windows
uses the configured ARM64 MSVC developer environment, `/std:c++17 /EHsc /W4
/WX /DMPV_STATIC` and the SDK include directory. Keep assertions enabled. Task
003c records compiler and platform evidence separately from application checks.

CI configures domain and FFmpeg jobs, Chromium/WebKit workflow checks, fake mpv and
synthetic Python controls, and desktop shell checks/builds on macOS 14, Windows
2022, and Ubuntu 24.04. It does not launch native application windows, test media
devices, create installers, sign artifacts, or establish release support. The
additional [Native build contracts](.github/workflows/native.yml) workflow builds
all three pinned mpv variants and compiles the Mac ANGLE, Windows and Linux
adapters. See [its exact scope](docs/native-runtime.md#ci-scope); configured jobs
are not evidence of a GitHub run until their actual results exist.

### Shell walkthrough

Tauri's Rust `main` creates the window and owns its event loop. Svelte mounts the editing workspace. `src/lib/client.ts` is the only IPC boundary; the handler delegates to a control worker. In the default build, media controls report that the native experiment is unavailable. The opt-in macOS feature attaches a GPU surface and loads libmpv. The domain remains the default Cargo workspace member.

### macOS GPU experiment (003b)

Install native development libraries with `brew install mpv pkg-config`. The macOS feature discovers libmpv with pkg-config; neither these libraries nor the native shim are required by the default shell or subtitle-core. This is not dependency packaging.

```sh
nvm use
npm ci
npm run tauri -- dev --features media-prototype -- --locked
```

Use File to open a video or audio file. Playback starts paused. Controls expose play/pause, exact seek in seconds, forward/backward step, an mpv audio-track ID (1 or 2 in the fixtures), and A–B looping. Open replaces the previous media session; Close unloads it. The native engine is reused until application shutdown. The editing workspace opens UTF-8 SRT files and commits text/timing edits through the Rust document boundary. Bounded historical desktop trials and current checks are summarized in [status](docs/status.md); native adoption remains open.

For the scripted GPU checks, first generate the fixtures as described in `tests/fixtures/README.md`, then launch from the repository root:

```sh
VSF_MEDIA_SMOKE="$PWD/tests/fixtures/generated/vfr-offset.mkv" npm run tauri -- dev --features media-prototype -- --locked
```

Keep the desktop session visible. The log must report the actual GPU renderer and `VSF GPU SMOKE: PASS`. The script checks VFR stepping, repeated seeks, track selection, loop wraps, replacement, stale command rejection, close and recovery from a missing path. It leaves the fixture open for visual confirmation. It does not replace checking the visible video, audio, focus, resize or overlays. Do not edit native source while measuring: the dev watcher restarts the process, which is not graceful-shutdown evidence.

```sh
cargo test --locked -p visualsubforge
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p visualsubforge --features media-prototype --all-targets -- -D warnings
npm run tauri -- build --debug --no-bundle --features media-prototype -- --locked
```

The Tauri macOS configuration uses an experimental macOS 11 deployment floor to avoid the installed SDK’s unsupported-libc++ warning. This is not a tested-runtime support claim; current runtime evidence is macOS 27 only.

The native shim uses Objective-C++17, ARC and the installed mpv headers. macOS OpenGL deprecation warnings are suppressed only for this experimental native file; the deprecation remains an architectural limitation. The [native contract](docs/native-runtime.md) records selected builds and remaining gates. Windows/Linux default builds compile the shell; their opt-in adapters are described below.

### Setup references

Setup references: [Tauri/Vite integration](https://v2.tauri.app/start/frontend/vite/), [Vite setup](https://vite.dev/guide/), [Svelte documentation](https://svelte.dev/docs/svelte/overview), and [Svelte ESLint setup](https://sveltejs.github.io/eslint-plugin-svelte/user-guide/). Native playback adoption and packaging remain open in the tracked status table.

The Ubuntu media-analysis job generates waveform and scene fixtures and runs all
11 FFmpeg integrations. Configured CI has not been executed from this checkout.

For the short 1080p/4K software-versus-hardware-copy decoding comparison, use the commands and limits in [compressed playback fixtures](tests/fixtures/README.md#compressed-playback-fixtures--task-003b). The UI exposes the same decoder switch and reports the selected decoder plus drop counters. A request for hardware decoding can fall back; inspect the reported decoder.

For repeated 4K looping and replacement/close races, see [lifecycle stress checks](tests/fixtures/README.md#repeated-playback-and-lifecycle-stress--task-003b). `VSF_MEDIA_SOAK` is an opt-in desktop diagnostic, not a headless test or a hard cancellation guarantee.

For comparable closed-state memory observations, `VSF_MEDIA_MEMORY` runs 50 open/seek/close cycles per decoder mode and samples the app's RSS. See [memory sampling](tests/fixtures/README.md#closed-state-memory-sampling--task-003b) for the exact command and limitations; successful completion is not a leak-freedom or memory-budget claim.

For the current workspace ownership walkthrough, see [frontend ownership](src/lib/README.md). The original path-entry experiment controls have been replaced by native file dialogs and menus; decode preference lives under Playback Diagnostics.

For the opt-in audio clock/loop-clear correction, use the [pinned mpv preparation recipe](tests/native/mpv-clock/README.md) and [native contract](docs/native-runtime.md). The recipe now pins upstream `master` commit `c1529642089bfebfc928a1c1664638a7a5d219ba` plus ten local patches; it never tracks a moving branch during preparation. Loading that candidate explicitly enables its capability-gated transport path; ordinary Homebrew playback still uses the known legacy clock. `VSF_AUDIO_SMOKE` fails if the required capability is absent. The recipe checks ancestry for the two upstream [CoreAudio cleanup prerequisites](tests/native/mpv-coreaudio-cleanup/README.md), including the formerly local handle-ownership correction. This is a dependency experiment, not an installed-library replacement or release dependency selection.


Render-API development builds must include the [pinned upstream fence correction](tests/native/mpv-render/README.md). The tested mpv 0.41.0 release leaks a GL fence per render; installing development packages alone does not establish that the correction is present. The current pinned upstream base already contains the fence correction. The preparation recipe supports unmodified upstream for rendering alone or that base plus the separate audio experiment. Existing prepared libraries are unchanged. Build privately and select the resulting library explicitly with process-local `DYLD_LIBRARY_PATH` on Mac or `LD_LIBRARY_PATH` on Linux; preserve ANGLE's runtime path when needed. Do not overwrite installed libraries. See [the native contract](docs/native-runtime.md) for tested builds and limits.

### macOS ANGLE/Metal application experiment

The `macos-angle` feature includes `media-prototype` and uses the existing
native boundary with an opaque webview. Use the [Mac build guide](docs/build-macos.md)
and [SDK preflight](docs/native-runtime.md#sdk-preflight-and-a-fresh-application-build)
for pinned ANGLE headers/libraries, matching mpv link/runtime selection and a
fresh application target. The feature does not download ANGLE or bundle its
libraries. The previous `/tmp/vsf-angle-source` has surviving libraries but
missing headers; it is not a complete SDK. A cached compiler pass cannot repair it.

Development runtime paths resolve relative library install names for a process;
they are not redistributable packaging. The ordinary `media-prototype` feature
retains the legacy Mac path. Both configurations remain opt-in experiments.

The native media experiment now previews the accepted SRT document on video. Open media and subtitles normally; Apply/Undo/Redo update the native preview, including while paused, and Close Subtitles removes it. Unapplied text drafts remain in the editor. The preview uses public mpv APIs and does not require `tests/native/mpv-subtitle-reuse` patches. Mac selects installed Hiragino Sans for the tested Unicode fixture. A serialized document over 4 MiB reports a preview error while editing/saving remain available. The [status table](docs/status.md) distinguishes bounded historical preview/editing trials from open font, scale-transition and update-performance gates.

### Workflow performance observation

For task006 performance observation, launch a built app with the process-local
environment variable `VSF_WORKFLOW_TRACE=1`. Stderr records JSON lines prefixed
`VSF_WORKFLOW_TRACE`: `document-acknowledgment` measures the typed client request
through IPC response decoding, and `preview-command` measures accepted-preview
serialization and native replacement commands. Records include operation,
generation/revision and durations, without subtitle text or paths. Tracing is
off by default. Its configuration request is asynchronous, so early requests
before that response can be absent; verify sample counts against the workload.
Frontend/backend durations use their respective monotonic clocks. Wall-clock
timestamps support approximate correlation only. Neither event proves Svelte
paint, GPU presentation or speaker output. The performance tracker retains
those separate acceptance gates.

Records are formatted completely before writing to stderr so native renderer
messages do not interleave inside serde's fragmented JSON formatting. Validate
every trace record and the expected operation counts; malformed or missing
records invalidate latency accounting rather than being silently filtered out.

Explicit Apply-button clicks and textarea commit keydowns also emit `apply-input`
records. They contain the browser event timestamp, trusted-event flag, performance
time origin, a monotonic bracket around `Date.now()`, and generation/revision
before the attempted edit. Correlate with the subsequent successful edit ACK;
the input record alone does not prove acceptance. Native menu actions and other
shortcuts without a supplied DOM event are not covered. Logging is queued after
the synchronous handler dispatches its work, then written on a blocking worker.
No event object, key text, subtitle content or document snapshot is retained.

The [DOM event timestamp](https://dom.spec.whatwg.org/#dom-event-timestamp) is
relative high-resolution time; it is not a physical device timestamp. The
[performance time origin](https://www.w3.org/TR/hr-time-3/#time-origin) is not
guaranteed to track later wall-clock adjustments. Keep the raw clocks separate:
`validClock` checks finite, nonnegative values and event/handler ordering only.
It does not certify resolution, clock synchronization or trustworthy input.
Reject invalid or missing records for latency accounting. Before correlating
VM events with host screenshots, independently bound clock offsets and drift,
retain uncertainty, and verify event delivery and unique visible labels.
Do not subtract an assumed automation delay or treat handler/request completion
as proof of visible presentation.

For Mac native-render attribution, add `VSF_RENDER_TRACE=1` to the process
environment. The render worker preallocates at most 16,384 stage records and
dumps `VSF_RENDER_FRAME` lines after graphics teardown; the summary reports
overflow. Allocation failure disables only the diagnostic. No frame-time IO,
decoded pixels or subtitle text enter this trace. Fields split drawable-lock
wait, update/setup, render, clear, presentation-call and report-swap stages.
Render time includes mpv's normal presentation-target wait; it is not GPU time.
`swapped=1` means the adapter completed its presentation call, not that the
compositor displayed a specific document revision. Begin/end Unix times use
one wall-clock anchor plus monotonic elapsed time for approximate correlation
with workflow traces; wall-clock changes invalidate that correlation. Abrupt
termination loses the buffered trace. Normal runs without the variable allocate
no trace buffer or collect timing clocks. This diagnostic currently covers the
Mac ANGLE and legacy OpenGL adapters only.

For actual Metal presentation feedback on the ANGLE path, also set
`VSF_PRESENT_TRACE=1`. An opt-in CAMetalLayer subclass forwards drawable
acquisition unchanged and registers the public Metal presentation callback.
`VSF_METAL_PRESENT` records drawable IDs, acquisition, presentation and callback
host times; `VSF_METAL_TRACE` reports pending callbacks and overflow. A zero
presentation time means unpresented/dropped, not success. Callback arrival is
not the presentation time. Two `VSF_METAL_CLOCK`/`VSF_METAL_CLOCK_END` anchors
bracket wall-clock reads with host-clock reads; reject changed wall-clock offsets
before correlating with input timestamps. The 16,384-slot buffer is shared only
to keep late callbacks safe; callbacks write distinct slots and publish completion
atomically. The render worker dumps after graphics/lock teardown without waiting
for callbacks. Pending slots must not be counted as presented. Disabled runs use
the original layer class and allocate no presentation buffer. Abrupt termination
loses the trace. This adds no pixel readback or native dependency patch; it does
not tag a drawable with a document revision. Match controlled paused edits to
unique visible screenshot labels and retain all intervening draws before making
any claim about subtitle presentation latency.

### Windows native playback experiment

The opt-in Windows adapter uses the same `media-prototype` feature. Set `VSF_MPV_SDK` to a development SDK containing `include/mpv/client.h` and an MSVC-compatible `lib/mpv.lib` for the target architecture. Build with:

```sh
npm run tauri -- build --debug --no-bundle --features media-prototype -- --locked
```

The imported libmpv DLL and all its dependencies must resolve when launching the executable. Historical ARM64 VM trials used specifically recorded private runtimes. Do not assume an old eight-patch artifact matches the current ten-patch baseline or thirteen-patch candidate. Use process-local dependency PATH and [record loaded binary identity](docs/native-runtime.md#identify-the-runtime-actually-loaded); this is not portable packaging. GGML unload is a separate experiment, not part of the selected mpv recipe. Do not mix architectures or silently substitute another runtime. `VSF_MEDIA_SMOKE` runs the common media sequence; `VSF_DIAGNOSTIC_EXIT=1` requests normal ordered teardown afterward. Windows checks video-output configuration rather than GPU swaps, which this adapter does not measure. Visible video and interactive layout/focus/modal acceptance remain separate checks.


### Linux native playback experiment

Linux file dialogs use the pinned ashpd desktop-portal adapter.
Run with a working session bus, `xdg-desktop-portal`, and a FileChooser backend
for the desktop (GNOME in the supplied VM). No `GTK_USE_PORTAL` override is
needed. The portal and desktop backend are required; there is no Zenity fallback.
The adapter reports missing-backend/request failures separately from cancellation.
The previous plugin implementation silently cancelled when both backends were
unavailable; its retained failure evidence prompted this change. The replacement adapter passes bounded Wayland and forced-scale XWayland
open/save/cancel checks and visible initial missing-service reporting. Pending
requests now report portal owner loss/replacement and bus closure rather than
leaving the editor busy. The [private-bus lifecycle regression](tests/native/linux-dialogs/README.md)
covers success, cancellation, disconnect, replacement, bus loss and same-process
retry after restart. It requires Linux, dbus-daemon and system Python's Gio;
ordinary Rust tests skip it. Fuller native parenting/modality checks remain open.
The local portal architecture decision remains proposed; bounded passes do not
close the remaining native parenting/modality gates.
This does not remove GTK/WebKit build prerequisites.

The `media-prototype` feature uses pkg-config to find `gtk+-3.0`, `epoxy`, and `mpv`, in addition to the Tauri development prerequisites above. On Ubuntu these are provided by `libgtk-3-dev`, `libepoxy-dev`, `libmpv-dev`, and `pkg-config`. The supplied Ubuntu 26.04 ARM64 VM uses the pinned Rust/Node versions and native system development packages; the [status table](docs/status.md) records the current compiler evidence and limits. Build with:

```sh
npm run tauri -- build --debug --no-bundle --features media-prototype -- --locked
```

Linux also supports the [playback/lifecycle soak](tests/fixtures/README.md#repeated-playback-and-lifecycle-stress--task-003b).
Use its explicit software-only mode when hardware decoding is unavailable;
that result excludes hardware coverage and counts render submissions rather
than compositor swaps. Select the corrected private render library as described
above, and use the desktop build command so frontend assets are embedded.

Run from a graphical user session. `VSF_MEDIA_SMOKE` and `VSF_DIAGNOSTIC_EXIT=1` exercise the common playback sequence and ordered exit. `GDK_BACKEND=wayland` or `GDK_BACKEND=x11` selects the available GTK backend; XWayland evidence is not a native Xorg session test. Linux readiness requires both output configuration and a successful render submission. These submissions are not compositor swaps or proof of device presentation. The tested system libmpv does not provide the experimental queued audio-clock capability; its known legacy transport limitations remain visible. Packaging and interactive editing acceptance are separate gates.

For the Linux numeric-locale startup regression (requires a graphical session),
run the separate-process example with the same native feature:

```sh
LC_ALL=en_GB.UTF-8 cargo run --locked -p visualsubforge --features media-prototype --example linux-locale -- en_GB.UTF-8
```

Choose an installed locale (`locale -a`); the argument is its expected LC_CTYPE
name. This checks the production startup helper through GTK initialization and a
worker, including an unchanged process environment. It does not load media or
establish desktop playback. Normal Linux media launches now initialize numeric
locale in application startup; explicit `LC_NUMERIC=C` test overrides must be
removed when validating this behavior. See [current status](docs/status.md) for evidence scope.
