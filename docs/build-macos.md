# Build and run VSF on the development Mac

This is the complete local Apple Silicon development path: native libraries,
Rust/Svelte application, and launch with the selected audio runtime. It is not a
redistributable package. Audio acceptance is still in progress under task 005.
The user-selected mpv source is pinned in `scripts/prepare-mpv-clock.py`; normal
builds never follow a moving upstream branch.

## Try the existing development build

On the current Mac, the 2026-10-05 candidate and app are already built. From
the repository root, this launches the exact runtime used by that checkpoint:

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
limits are recorded in [task 005](tasks/005-audio-loop-timing.md#upstream-audio-integration-checkpoint--2026-10-05).

## Prerequisites

Use the Rust/Node versions and Xcode setup in [CONTRIBUTING](../CONTRIBUTING.md#commands-and-toolchains).
On the current development Mac those tools, Homebrew media dependencies, and
ANGLE are already installed. For another Mac, the native build needs:

```sh
brew install mpv pkg-config meson ninja
```

Homebrew supplies mpv's development dependencies, including FFmpeg, libass and
libplacebo. These transitive versions are not locked by VSF; Meson checks their
minimum versions. The installed Homebrew player is not our patched runtime.
Full Xcode and its Metal Toolchain are needed to build ANGLE.

## ANGLE / Metal

On this Mac, reuse the existing `/tmp/vsf-angle-source` build. It is pinned to
`0ed62f49b5e02cc5e6db4a175a98f545cc994715`, with depot_tools revision
`8a5434051036b32412a2ecb10c213a72e3f3ccb9`. Temporary directories can disappear;
for a fresh checkout, follow ANGLE's [official setup](https://chromium.googlesource.com/angle/angle/+/main/doc/DevSetup.md)
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
installation on another machine. Both `out/Metal/libEGL.dylib` and
`out/Metal/libGLESv2.dylib` must exist.

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

The existing isolated tools on this Mac can be selected, if needed, with
`export PATH="/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH"`.

## Build the application

Continue in the same shell, from the VSF repository root:

```sh
nvm use
npm ci
export VSF_ANGLE_SDK=/tmp/vsf-angle-source
export PKG_CONFIG_PATH="$VSF_NATIVE/build/meson-uninstalled"
export DYLD_LIBRARY_PATH="$VSF_NATIVE/build:$VSF_ANGLE_SDK/out/Metal"
pkg-config --variable=prefix mpv
cargo test --locked --workspace --features macos-angle
npm run tauri -- build --debug --bundles app --features macos-angle -- --locked
```

The printed pkg-config prefix must be the selected mpv build directory. This
selects its headers/link library; `DYLD_LIBRARY_PATH` independently selects the
runtime libraries for the launched process. Keep both settings. They do not
change your installed libraries.

## Launch and try editing

```sh
"$VSF_REPO/target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge"
```

Launch that executable from this shell so it inherits the runtime paths. Finder
launch does not inherit them and is not the supported local-build path yet.
Use File to open a video or audio file. Click **Create cue** to start new
subtitles, or open an existing UTF-8 SRT. New subtitles ask for a filename and
location on first Save. Scroll horizontally on the waveform to pan, vertically
to zoom, and check **Follow playback** to resume following the playhead. Playback Diagnostics
should identify the experimental queued timeline. Edit text/timing, use
Apply/Undo/Redo, and save a copy of your subtitles. The native audio candidates
still have the limitations recorded in [task 005](tasks/005-audio-loop-timing.md).

For frontend iteration, use the same environment with:

```sh
npm run tauri -- dev --features macos-angle -- --locked
```

Quit the built app before starting another instance. Development mode uses port
1420 and can restart on source changes, so use the built app for playback tests.
Windows/Linux build instructions remain in [CONTRIBUTING](../CONTRIBUTING.md);
their candidate-runtime validation is tracked separately.
