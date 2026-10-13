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
| Linux outputs | Resolve PulseAudio backward clock steps (a retained run measured 98 microseconds against a 20.833-microsecond one-frame tolerance) and intermittent PipeWire failures; preserve captured-content and presentation distinctions. |
| Windows output | Resolve the retained loop-clear quiet interval and capture-versus-clock discrepancy; repeat affected full-editor workflows with the selected runtime. ARM64 VM evidence does not establish physical or x86-64 behavior. |
| Editor and lifetime | Verify real pointer/keyboard delivery, draft protection through dialogs, replacement, native resource cleanup and visible presentation. Upstream lifetime tests do not prove compositor/device behavior or absence of leaks. |
| Distribution | Complete dependency/license inventory, self-contained packaging, signing and the explicitly claimed OS/architecture/display/device matrix before release claims. |

These gates preserve the existing acceptance requirements. The local task
history contains dated investigations; a fresh tracked checkout can reproduce
the build/control commands above without those intentionally untracked files.
