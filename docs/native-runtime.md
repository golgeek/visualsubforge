# Native runtime maintenance and evidence

VSF maintains a pinned mpv variant behind its existing native media boundary.
The shared render/build correction, ten-patch audio baseline and thirteen-patch
audio candidate are distinct configurations. The audio configurations also
include the shared correction. They remain experimental. An upstream unit-test pass,
capability property or successful application build does not accept the runtime
for daily use or establish platform support.

VSF initializes mpv with automatic and built-in scripting disabled through
`mpv_client.hpp` on every platform. `config=no` alone does not disable mpv's
built-in scripts. VSF supplies its own controls and opens local files; these
scripts are unused, and LuaJIT execution caused a hardened-signature termination
in the Mac preview. The control owner applies this policy before `mpv_initialize`
and propagates option errors. Review the complete built-in list in upstream
`player/scripting.c` when updating mpv. Mac signing now exercises the packaged
core through a separately signed initialization probe, without JIT exceptions;
actual signed GUI playback remains a separate acceptance check.

## Maintenance ownership

The maintainer approving a dependency update owns its source provenance,
regression evidence and rollback path. Each update must name its reviewer and
record which patches remain local, which are upstream and which are excluded
experiments. Keep transport/property policy shared; Mac render workers, GTK
main-thread rendering and mpv-owned Windows rendering retain their necessary
platform differences. A new patch needs a concrete failure and regression, not
an attempt to make those native APIs identical.

The recipe manifests plus `scripts/prepare-mpv-clock.py` pin upstream
`c1529642089bfebfc928a1c1664638a7a5d219ba` and patch bytes. The rendering fix and two
CoreAudio cleanup prerequisites are already upstream; ancestry is checked.
The render series also guards mpv's Swift `MetalLayer` reference with
`HAVE_VULKAN`, matching its source-selection condition. This fixes compilation
with the Mac preview's OpenGL-only libplacebo while retaining the Vulkan path.
GGML unload, subtitle-renderer reuse, legacy VLC and WASAPI instrumentation are
separate experiments, not additions to the selected audio recipe.

## Repeatable source builds

Use Python 3.14.7 and the pinned tools in `tests/native/build-requirements.txt`
inside a virtual environment. The OS compiler, Xcode and media dependencies are
not hermetically pinned: Meson records their actual versions and build options.
On Mac, Homebrew supplies dependencies as described in [the build guide](build-macos.md).
Do not interpret a locked Rust/npm build as a locked native dependency graph.

```sh
python3 -m venv /tmp/vsf-native-build-tools
/tmp/vsf-native-build-tools/bin/python -m pip install --only-binary=:all: -r tests/native/build-requirements.txt
export PATH="/tmp/vsf-native-build-tools/bin:$PATH"
git clone https://github.com/mpv-player/mpv.git /tmp/vsf-mpv-upstream
python scripts/check-mpv-build.py --source /tmp/vsf-mpv-upstream --output /tmp/vsf-native-candidate --variant candidates
```

Use a new output directory for each run. Use `--variant render` or `baseline`
for controls. An existing upstream clone can be supplied read-only; committed
source is copied to an isolated checkout. The driver prepares, builds the player
and shared library, runs the declared upstream tests and records exact commands.
It fails on missing or duplicate results and requires the libmpv lifetime test;
audio variants also require queued-timeline and PCM-boundary tests to execute.
Explicit optional skips remain visible. No library is installed globally.

`build-receipt.json` records source-file hashes, patch manifests, compiler and
dependency metadata, results and produced binary hashes. A failed run retains
its directory and a failed receipt. Keep `commands.log` with it. Raw Meson
`testlog.json` can contain the inherited environment and must not be published;
the receipt copies only the necessary result fields.

## SDK preflight and a fresh application build

macOS ANGLE needs both public headers and `out/Metal/libEGL.dylib` and
`libGLESv2.dylib`. Windows needs matching mpv headers, an MSVC-compatible import
library and a runtime DLL/dependency set. Linux uses pkg-config for GTK, epoxy
and mpv. The Cargo build script checks required SDK files and watches them so
removing a header/library invalidates its cache.

The preflight rejects an incomplete SDK and records file identity before a
build. It also checks the selected pkg-config link directory, including Meson's
uninstalled configuration, and can verify libmpv against its build receipt.
For the Mac candidate above, after building a complete ANGLE SDK:

```sh
export VSF_ANGLE_SDK=/absolute/path/to/angle-sdk
export PKG_CONFIG_PATH=/tmp/vsf-native-candidate/build/meson-uninstalled
export DYLD_LIBRARY_PATH="/tmp/vsf-native-candidate/build:$VSF_ANGLE_SDK/out/Metal"
python scripts/native-preflight.py --feature macos-angle --mpv-runtime /tmp/vsf-native-candidate/build/libmpv.2.dylib --mpv-build-receipt /tmp/vsf-native-candidate/build-receipt.json --output /tmp/vsf-native-preflight.json
nvm use
npm ci
export CARGO_TARGET_DIR=/tmp/vsf-native-app-clean
npm run tauri -- build --debug --bundles app --features macos-angle -- --locked
```

Choose an unused Cargo target directory to validate compilation without stale
objects. Keep the preflight, build log and executable hash together. For another
platform use `--feature media-prototype`, its actual `.so` or DLL path and the
native build instructions in [CONTRIBUTING](../CONTRIBUTING.md).
`--application PATH` additionally hashes a built executable. A preflight is a
file check; the compiler/linker must still establish ABI/architecture compatibility.

`scripts/prepare-angle-headers.py --output NEW_DIRECTORY` downloads only the
public headers at `0ed62f49b5e02cc5e6db4a175a98f545cc994715`, with a pinned hash of
member names, kinds and file contents. Gitiles changes archive timestamps on
download; those timestamps are excluded, while downloaded archive hashes remain
in each manifest. `--archive PATH` supports offline reuse. This is for compiler checks or
restoring the headers of that exact development version. It does not build
ANGLE or turn an arbitrary pair of libraries into a verified SDK. A fresh
ANGLE library build still requires the pinned depot_tools and GN/Ninja path in
the Mac guide. Retain provenance and hashes when reusing existing libraries.

## Identify the runtime actually loaded

Link inputs and runtime search paths can select different libraries. After
launching the built app with its explicit development environment, record the
owned application's PID while it is live:

```sh
python scripts/record-native-runtime.py --pid APP_PID --expect APP_EXECUTABLE --expect LIBMPV_PATH --expect ANGLE_EGL_PATH --expect ANGLE_GLES_PATH --output NEW_RUNTIME_JSON
```

Omit the ANGLE expectations on Windows/Linux. This uses macOS `lsof`, Linux
`/proc` maps or Windows process modules; it never reads application memory or
changes system library search paths. Required images must be observed with
matching backing-file hashes, and the process identity must survive inspection.
Unavailable images are reported. macOS system shared-cache images may have no
individual file to hash. The receipt is an observation of mapped file names and
their disk contents, not proof that already-mapped pages were never changed.
Retain OS/architecture and the app's renderer/audio capability log too.

## CI scope

`Native build contracts` prepares and builds all three mpv variants on macOS,
runs upstream tests and retains receipts. Its adapter matrix compiles the Mac
ANGLE, Windows and Linux C++ adapters against the pinned public mpv API. The Mac
ANGLE lane uses pinned headers without linking ANGLE; Windows compiles an object
without linking a runtime; Linux additionally compiles the native Rust boundary
against system development packages. These are explicit compiler checks, not
three-platform playback. The existing `Checks` workflow separately builds the
default desktop application on three OSes and the legacy Mac native application.
No CI lane establishes actual GPU/audio/input acceptance.

## Recorded validation (2026-10-11 UTC)

On the Apple M3 Max development host (macOS 27, arm64), isolated builds of the
pinned render, baseline and candidate variants passed **37, 39 and 39** declared
upstream tests respectively, with no skips. Meson 1.12.1, Ninja 1.13.2 and Python
3.14.7 were used. The final candidate run used the current receipt driver.
Compiler, dependency and binary identities are retained in each local receipt;
the package graph remains observed rather than locked.

A fresh Cargo target built the selected `macos-angle` application and app bundle.
Its workspace tests passed **99** cases and selected-feature Clippy passed.
The existing ANGLE libraries were reused with restored pinned public headers;
this was **not a fresh ANGLE library build**. Both the Python preflight and Cargo
build guard rejected the incomplete old SDK. The ANGLE C++ compiler check also
passed against freshly downloaded, content-verified headers.

The bundled app showed video on the M3 Max, completed `VSF GPU SMOKE`, and quit
normally with exit code 0. The final recorder observed the required executable,
libmpv, EGL and GLES files among 49 images, with none unavailable. A negative
control requiring an unmapped file failed. These are bounded command, visible
video, identity and shutdown observations; no acoustic/content acceptance follows.

Ubuntu 26.04 ARM64 VM compilation passed for the Linux C++ adapter and
`cargo check --locked -p media-player --features native-mpv`. This run did not
launch its editor or exercise GPU/audio. The Python runner passed **66 synthetic
controls across ten suites**, including receipt validation, ANGLE content identity
and runtime-file filtering. No Git remote is configured in the development
checkout, so these are local results, not a GitHub Actions run. Windows 11 ARM64
MSVC object compilation also passed with the pinned public mpv header; adapter
and shared helper bytes were checked against this checkout. That is neither
a Windows link nor a runtime test. Linux/Windows runtime-recorder execution
remains unverified; the CI Windows x64 environment has not run locally.

## Upgrade checklist

1. Reproduce the motivating defect with the previous runtime. Keep its source,
   binary hashes and expected failure. Name the owner/reviewer of the update.
2. Select an immutable upstream revision. Review patch ancestry and semantics,
   update hashes deliberately and remove redundant application of upstream fixes.
   Preserve excluded experimental evidence instead of silently adding patches.
3. Build render, baseline and candidate controls with the same dependency graph.
   Compare receipts, run upstream tests and the repository's synthetic oracle
   controls. Require meaningful execution, not just successful test discovery.
4. Preflight and rebuild each claimed native application configuration. Record
   its loaded library identities, not only its link flags or capability marker.
5. Repeat affected editor, transport, content, replacement and shutdown checks
   on each claimed output backend. Keep device/VM, architecture, renderer and
   rate distinctions. Do not repair a clock failure by relaxing its threshold.
6. Record remaining gates, licensing/packaging implications and rollback to the
   previous isolated runtime. Adoption requires the affected gates to pass;
   compilation or source preparation alone cannot close them.

## Remaining adoption gates

| Area | Required evidence |
| --- | --- |
| Candidate captured content | Rate changes, outside-range re-entry and replacement, including near-B and paused cases, on the final built runtime. Earlier isolated traces cannot be reassigned to it. |
| CoreAudio and AVFoundation | Validate candidate continuation with its defined backend/rate scope; resolve retained AVFoundation discontinuity and EOF/content limits. A successful property transaction is insufficient. |
| Linux outputs | Resolve PulseAudio backward clock steps (the current VM run reaches8.698ms against a20.833µs one-frame tolerance) and intermittent PipeWire failures. An existing interpolation-option comparison passes six short trials but still needs broader content/editor validation. Preserve captured-content and presentation distinctions. |
| Windows output | Resolve the retained loop-clear quiet interval and capture-versus-clock discrepancy; repeat affected full-editor workflows with the selected runtime. ARM64 VM evidence does not establish physical or x86-64 behavior. |
| Editor and lifetime | Verify real pointer/keyboard delivery, draft protection through dialogs, replacement, native resource cleanup and visible presentation. Upstream lifetime tests do not prove compositor/device behavior or absence of leaks. |
| Distribution | Complete dependency/license inventory, self-contained packaging, signing and the explicitly claimed OS/architecture/display/device matrix before release claims. |

These gates preserve the existing acceptance requirements. The local task
history contains dated investigations; a fresh tracked checkout can reproduce
the build/control commands above without those intentionally untracked files.

The [2026-10-18 CoreAudio audit](native-reliability.md#current-coreaudio-clear-boundary-audit--2026-10-18)
adds exact-candidate 1× captures and a separately identified diagnostic build's
constant-rate/phase/pause submission and process-capture evidence. It retains four
ambiguous combined-reference comparisons and records their separate ordered-boundary
checks. It does not substitute for the changing-rate, re-entry/replacement,
other-output, full-editor or physical presentation gates above.

The subsequent [rate-change and editor audit](native-reliability.md#rate-change-clear-audit--2026-10-18)
adds immediate 0.5×↔1.5× change/clear evidence and actual Mac editor re-entry/
replacement captures. It also reproduces an 8.6875ms quiet interval in the
uninstrumented candidate. These bounded passes do not close the retained
interruption, sustained rate/format transitions or other-output/device gates.

## Windows native playback experiment

Run from the repository root after [common setup](../CONTRIBUTING.md#prerequisites)
and `npm ci`. The Tauri build command builds frontend assets before Rust.

The opt-in Windows adapter uses the same `media-prototype` feature. Set `VSF_MPV_SDK` to a development SDK containing `include/mpv/client.h` and an MSVC-compatible `lib/mpv.lib` for the target architecture. Build with:

```sh
npm run tauri -- build --debug --no-bundle --features media-prototype -- --locked
```

The imported libmpv DLL and all its dependencies must resolve when launching the executable. Historical ARM64 VM trials used specifically recorded private runtimes. Do not assume an old eight-patch artifact matches the current ten-patch baseline or thirteen-patch candidate. Use process-local dependency PATH and [record loaded binary identity](native-runtime.md#identify-the-runtime-actually-loaded); this is not portable packaging. GGML unload is a separate experiment, not part of the selected mpv recipe. Do not mix architectures or silently substitute another runtime. `VSF_MEDIA_SMOKE` runs the common media sequence; `VSF_DIAGNOSTIC_EXIT=1` requests normal ordered teardown afterward. Windows checks video-output configuration rather than GPU swaps, which this adapter does not measure. Visible video and interactive layout/focus/modal acceptance remain separate checks.

## Linux native playback experiment

Run from the repository root after [common setup](../CONTRIBUTING.md#prerequisites)
and `npm ci`. Build frontend assets with `npm run build` before direct Cargo
compilation; the Tauri build command also performs that prerequisite.

Linux file dialogs use the pinned ashpd desktop-portal adapter.
Run with a working session bus, `xdg-desktop-portal`, and a FileChooser backend
for the desktop (GNOME in the supplied VM). No `GTK_USE_PORTAL` override is
needed. The portal and desktop backend are required; there is no Zenity fallback.
The adapter reports missing-backend/request failures separately from cancellation.
The previous plugin implementation silently cancelled when both backends were
unavailable; its retained failure evidence prompted this change. The replacement adapter passes bounded Wayland and forced-scale XWayland
open/save/cancel checks and visible initial missing-service reporting. Pending
requests now report portal owner loss/replacement and bus closure rather than
leaving the editor busy. The [private-bus lifecycle regression](../tests/native/linux-dialogs/README.md)
covers success, cancellation, disconnect, replacement, bus loss and same-process
retry after restart. It requires Linux, dbus-daemon and system Python's Gio;
ordinary Rust tests skip it. Fuller native parenting/modality checks remain open.
The local portal architecture decision remains proposed; bounded passes do not
close the remaining native parenting/modality gates.
This does not remove GTK/WebKit build prerequisites.

The `media-prototype` feature uses pkg-config to find `gtk+-3.0`, `epoxy`, and `mpv`, in addition to the [Tauri development prerequisites](../CONTRIBUTING.md#prerequisites). On Ubuntu these are provided by `libgtk-3-dev`, `libepoxy-dev`, `libmpv-dev`, and `pkg-config`. The supplied Ubuntu 26.04 ARM64 VM uses the pinned Rust/Node versions and native system development packages; the [status table](status.md) records the current compiler evidence and limits. Build with:

```sh
npm run tauri -- build --debug --no-bundle --features media-prototype -- --locked
```

Linux also supports the [playback/lifecycle soak](../tests/fixtures/README.md#repeated-playback-and-lifecycle-stress--task-003b).
Use its explicit software-only mode when hardware decoding is unavailable;
that result excludes hardware coverage and counts render submissions rather
than compositor swaps. Select the corrected private render library using
[development runtime selection](#development-runtime-selection-notes), and use the desktop build command so frontend assets are embedded.

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
removed when validating this behavior. See [current status](status.md) for evidence scope.

## Development runtime selection notes

For the opt-in audio clock/loop-clear correction, use the [pinned mpv preparation recipe](../tests/native/mpv-clock/README.md) and [native contract](native-runtime.md). The recipe now pins upstream `master` commit `c1529642089bfebfc928a1c1664638a7a5d219ba` plus ten local patches; it never tracks a moving branch during preparation. Loading that candidate explicitly enables its capability-gated transport path; ordinary Homebrew playback still uses the known legacy clock. `VSF_AUDIO_SMOKE` fails if the required capability is absent. The recipe checks ancestry for the two upstream [CoreAudio cleanup prerequisites](../tests/native/mpv-coreaudio-cleanup/README.md), including the formerly local handle-ownership correction. This is a dependency experiment, not an installed-library replacement or release dependency selection.


Render-API development builds must include the [pinned upstream fence correction](../tests/native/mpv-render/README.md). The tested mpv 0.41.0 release leaks a GL fence per render; installing development packages alone does not establish that the correction is present. The current pinned upstream base already contains the fence correction. The preparation recipe supports unmodified upstream for rendering alone or that base plus the separate audio experiment. Existing prepared libraries are unchanged. Build privately and select the resulting library explicitly with process-local `DYLD_LIBRARY_PATH` on Mac or `LD_LIBRARY_PATH` on Linux; preserve ANGLE's runtime path when needed. Do not overwrite installed libraries. See [the native contract](native-runtime.md) for tested builds and limits.

### macOS ANGLE/Metal application experiment

The `macos-angle` feature includes `media-prototype` and uses the existing
native boundary with an opaque webview. Use the [Mac build guide](build-macos.md)
and [SDK preflight](native-runtime.md#sdk-preflight-and-a-fresh-application-build)
for pinned ANGLE headers/libraries, matching mpv link/runtime selection and a
fresh application target. The feature does not download ANGLE or bundle its
libraries. The previous `/tmp/vsf-angle-source` has surviving libraries but
missing headers; it is not a complete SDK. A cached compiler pass cannot repair it.

Development runtime paths resolve relative library install names for a process;
they are not redistributable packaging. The ordinary `media-prototype` feature
retains the legacy Mac path. Both configurations remain opt-in experiments.

The native media experiment now previews the accepted SRT document on video. Open media and subtitles normally; Apply/Undo/Redo update the native preview, including while paused, and Close Subtitles removes it. Unapplied text drafts remain in the editor. The preview uses public mpv APIs and does not require `tests/native/mpv-subtitle-reuse` patches. Mac selects installed Hiragino Sans for the tested Unicode fixture. A serialized document over 4 MiB reports a preview error while editing/saving remain available. The [status table](status.md) distinguishes bounded historical preview/editing trials from open font, scale-transition and update-performance gates.

## Historical implementation baseline

The table records review-remediation results, not a fresh-machine installation or a
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
