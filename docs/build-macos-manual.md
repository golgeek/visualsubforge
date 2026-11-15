# Manual macOS native build recipes

For normal local testing, use [the one-command build](build-macos.md).
This reference documents the individual Apple Silicon development stages: native libraries,
Rust/Svelte application, and launch with the selected audio runtime. It is not a
redistributable package. Audio/content acceptance remains open in the
[native runtime contract](native-runtime.md#remaining-adoption-gates).
The user-selected mpv source is pinned in `scripts/prepare-mpv-clock.py`; normal
builds never follow a moving upstream branch.

For repeatable validation, use the [native maintenance contract](native-runtime.md):
it provides a fresh source-build driver, SDK preflight, build receipts, loaded
binary identification and the upgrade/adoption gates. Existing libraries below
are reusable development artifacts, not proof of a fresh dependency build.

## Historical local launch example (2026-10-05)

The 2026-10-05 checkpoint used the paths below. They are not a current-build
guarantee: later source builds can replace the application, and temporary SDK
headers can disappear while libraries survive. Use this only when those exact
artifacts are still present and their recorded identities match:

```sh
export VSF_REPO="$PWD"
export VSF_NATIVE="$VSF_REPO/tests/fixtures/generated/audio-integration-master-v1"
export VSF_ANGLE_SDK=/tmp/vsf-angle-source
export DYLD_LIBRARY_PATH="$VSF_NATIVE/build:$VSF_ANGLE_SDK/out/Metal"
"$VSF_REPO/target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge"
```

These generated/temporary files are local, not distributed with the repository.
If they are missing, or after changing source, follow the build instructions
below. Do not set diagnostic `VSF_*_SMOKE` or `VSF_DIAGNOSTIC_EXIT` variables
when launching interactively. The app remains experimental; the audio acceptance
limits are recorded in the [native runtime contract](native-runtime.md#remaining-adoption-gates).

## Prerequisites

Use the Rust/Node versions and Xcode setup in [CONTRIBUTING](../CONTRIBUTING.md#commands-and-toolchains).
Install the native development dependencies; existing installations still need
the completeness and identity checks below:

```sh
brew install mpv pkg-config meson ninja
```

Homebrew supplies mpv's development dependencies, including FFmpeg, libass and
libplacebo. These transitive versions are not locked by VSF; Meson checks their
minimum versions. The installed Homebrew player is not our patched runtime.
Full Xcode and its Metal Toolchain are needed to build ANGLE.

## ANGLE / Metal

The development ANGLE libraries were built from
`0ed62f49b5e02cc5e6db4a175a98f545cc994715`, with depot_tools revision
`8a5434051036b32412a2ecb10c213a72e3f3ccb9`. Temporary directories can disappear;
the old `/tmp/vsf-angle-source` is currently incomplete. For a fresh checkout, follow ANGLE's [official setup](https://chromium.googlesource.com/angle/angle/+/main/doc/DevSetup.md)
using those revisions. Our `.gclient` solution is:

```python
solutions = [{"name": ".", "url": "https://chromium.googlesource.com/angle/angle.git",
              "managed": False, "custom_vars": {"checkout_angle_cl_deps": False,
              "checkout_angle_dawn_deps": False, "checkout_angle_restricted_traces": False},
              "custom_deps": {}}]
```

From that checkout, with depot_tools on PATH:

```sh
gclient sync --no-history --nohooks
# Only if Xcode reports that its Metal Toolchain is missing:
xcodebuild -downloadComponent MetalToolchain
buildtools/mac/gn gen out/Metal --args='is_debug=false target_cpu="arm64" angle_enable_gl=false angle_enable_vulkan=false angle_enable_metal=true angle_enable_wgpu=false angle_build_tests=false use_system_xcode=true use_remoteexec=false symbol_level=0 is_component_build=false enable_rust=false'
third_party/ninja/ninja -C out/Metal -j 6 libEGL libGLESv2
```

The development evidence covers the existing pinned checkout/build, not a fresh
installation on another machine. The public `include/` headers and both
`out/Metal/libEGL.dylib` and `out/Metal/libGLESv2.dylib` must exist. A temporary
directory can retain libraries after its source/header files disappear; run
preflight instead of inferring SDK completeness from the library files alone.

## Prepare and build mpv

Run the following from the VSF repository root. Choose a new output directory
for each changed recipe; preparation refuses to overwrite previous evidence.

```sh
export VSF_REPO="$PWD"
export VSF_NATIVE="$VSF_REPO/tests/fixtures/generated/local-audio-build"
mkdir -p "$VSF_REPO/tests/fixtures/generated"
git clone https://github.com/mpv-player/mpv.git "$VSF_REPO/tests/fixtures/generated/mpv-upstream"
python3 scripts/prepare-mpv-clock.py \
  --source "$VSF_REPO/tests/fixtures/generated/mpv-upstream" \
  --output "$VSF_NATIVE" --audio-candidates
meson setup "$VSF_NATIVE/build" "$VSF_NATIVE/source" \
  -Dlibmpv=true -Dcocoa=enabled -Dswift-build=enabled \
  -Dmacos-media-player=disabled -Dtests=true \
  -Dmanpage-build=disabled -Dvapoursynth=disabled
meson compile -C "$VSF_NATIVE/build" -j 6 mpv:executable mpv:shared_library
meson test -C "$VSF_NATIVE/build" --print-errorlogs -j 6
```

Reuse an existing upstream clone instead of cloning again; it must contain the
pinned commit. `--audio-candidates` explicitly includes the three unfinished
audio corrections; omit it for the ten-patch baseline. Both are experimental.
The optional VapourSynth filter is disabled because the tested environment lacks
its required header. No `meson install` or system-library replacement is needed.

For pinned Meson/Ninja versions and a retained build receipt, prefer the
[automated recipe driver](native-runtime.md#repeatable-source-builds). The manual
commands above show the same build options but do not produce that receipt.

## Build the application

Continue in the same shell, from the VSF repository root:

```sh
nvm use
npm ci
npm run build
export VSF_ANGLE_SDK=/absolute/path/to/complete-angle-sdk
export PKG_CONFIG_PATH="$VSF_NATIVE/build/meson-uninstalled"
export DYLD_LIBRARY_PATH="$VSF_NATIVE/build:$VSF_ANGLE_SDK/out/Metal"
python3 scripts/native-preflight.py --feature macos-angle --mpv-runtime "$VSF_NATIVE/build/libmpv.2.dylib" --output "$VSF_NATIVE/app-preflight.json"
pkg-config --cflags --libs mpv
cargo test --locked --workspace --features macos-angle
npm run tauri -- build --debug --bundles app --features macos-angle -- --locked
```

Verify the printed include paths and `-L` library search directory select your
prepared source/build. Meson's uninstalled pkg-config file can retain a future
installation prefix, so `--variable=prefix` alone is insufficient. Preflight
refuses to overwrite its receipt; choose a new output filename for a later run.
For a clean rebuild, also choose an unused `CARGO_TARGET_DIR` as described in
the native contract. These flags select headers/link inputs; `DYLD_LIBRARY_PATH` independently selects the
runtime libraries for the launched process. Keep both settings. They do not
change your installed libraries.

## Launch and try editing

```sh
"$VSF_REPO/target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge"
```

Launch that executable from this shell so it inherits the runtime paths. Finder
launch does not inherit them and is not the supported local-build path yet.
Use File to open a video or audio file. Choose **New subtitles** on Start to begin, or open an existing UTF-8 SRT. New subtitles ask for a filename and
location on first Save. Scroll horizontally on the waveform to pan, vertically
to zoom, and check **Follow playback** to resume following the playhead. Playback Diagnostics
should identify the experimental queued timeline. Edit text/timing, try automatic text commits and Undo/Redo, and save a copy of your subtitles. The native audio candidates
still have the limitations recorded in the [native contract](native-runtime.md#remaining-adoption-gates).

For frontend iteration, use the same environment with:

```sh
npm run tauri -- dev --features macos-angle -- --locked
```

Quit the built app before starting another instance. Development mode uses port
1420 and can restart on source changes, so use the built app for playback tests.
Windows/Linux build instructions are in the [native runtime guide](native-runtime.md);
their candidate-runtime validation is tracked separately.

## Legacy OpenGL development and smoke checks

Install native development libraries with `brew install mpv pkg-config`. The macOS feature discovers libmpv with pkg-config; neither these libraries nor the native shim are required by the default shell or subtitle-core. This is not dependency packaging.

```sh
nvm use
npm ci
npm run tauri -- dev --features media-prototype -- --locked
```

Use File to open a video or audio file. Playback starts paused. Controls expose play/pause, exact seek in seconds, forward/backward step, an mpv audio-track ID (1 or 2 in the fixtures), and A–B looping. Open replaces the previous media session; Close unloads it. The native engine is reused until application shutdown. The editing workspace opens UTF-8 SRT files and commits text/timing edits through the Rust document boundary. Bounded historical desktop trials and current checks are summarized in [status](status.md); native adoption remains open.

For the scripted GPU checks, first generate the fixtures as described in `tests/fixtures/README.md`, then launch from the repository root:

```sh
VSF_MEDIA_SMOKE="$PWD/tests/fixtures/generated/vfr-offset.mkv" npm run tauri -- dev --features media-prototype -- --locked
```

Keep the desktop session visible. The log must report the actual GPU renderer and `VSF GPU SMOKE: PASS`. The script checks VFR stepping, repeated seeks, track selection, loop wraps, replacement, stale command rejection, close and recovery from a missing path. It leaves the fixture open for visual confirmation. It does not replace checking the visible video, audio, focus, resize or overlays. Do not edit native source while measuring: the dev watcher restarts the process, which is not graceful-shutdown evidence.

```sh
npm run build
cargo test --locked -p visualsubforge
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p visualsubforge --features media-prototype --all-targets -- -D warnings
npm run tauri -- build --debug --no-bundle --features media-prototype -- --locked
```

The Tauri macOS configuration uses an experimental macOS 11 deployment floor to avoid the installed SDK’s unsupported-libc++ warning. This is not a tested-runtime support claim; current runtime evidence is macOS 27 only.

The native shim uses Objective-C++17, ARC and the installed mpv headers. macOS OpenGL deprecation warnings are suppressed only for this experimental native file; the deprecation remains an architectural limitation. The [native contract](native-runtime.md) records selected builds and remaining gates. Windows/Linux default builds compile the shell; their opt-in adapters are described in the [native contract](native-runtime.md).
