# Build the native app

From the repository root, after the one-time setup below:

```sh
make build          # Optimized app with native media
make install        # Build and install it (optional)
make debug          # Native app with debug symbols and webview devtools
make debug install  # Install the debug app instead
```

All three platforms include native playback, FFmpeg/ffprobe and the plugin CLI.
`make` alone means `make build`. These are local builds for your own machine;
[release packaging](releases.md) additionally inventories sources/licenses and
handles release signing. Native media retains its [experimental limits](native-runtime.md).

| Platform | Finished build (`release` or `debug`) | `make install` destination |
| --- | --- | --- |
| macOS, Apple Silicon or Intel | `target/native/macos/release/package/VisualSubForge.app` | `~/Applications/VisualSubForge.app` |
| Windows x64 | `target/native/windows/release/package/VisualSubForge/visualsubforge.exe` | `%LOCALAPPDATA%\Programs\VisualSubForge`, with a Start menu shortcut |
| Ubuntu 24.04 amd64 | `target/native/linux/release/package/*.deb` | System package, through `sudo apt-get install` |

Open the `.app` or `.exe` directly, or launch VSF from the desktop after installing.
Quit an existing VSF instance before replacing its installation. On macOS and
Windows, `INSTALL_DIR=/path/to/apps make install` changes the parent install directory.
No administrator access is needed there. Windows requires WebView2 already installed.
Linux installation prompts for sudo; never run the build itself as root.

## Common tools

- GNU Make 3.81 or newer and Bash. On Windows use MSYS2's `make`, not `nmake`.
- Rustup (the repository selects Rust 1.98.1).
- Node 24.21.0 with npm 11.19.0. With POSIX nvm, run `nvm install && nvm use`.
- Native Python 3.11 or newer; validation uses 3.14.7. Use `PYTHON=python` if that
  is its command name. Python remains required by the existing SDK/package helpers;
  Make owns dependency ordering, app profiles, packaging and installation.
- Git and network access for source/dependency downloads. Use a checkout path
  without whitespace, as required by the native Make target paths.

The build installs pinned Meson/Ninja into a private virtual environment and
runs `npm ci`. OS development packages are installed once, explicitly below.

## macOS setup

Select full Xcode (not just Command Line Tools), complete its first-launch setup,
and install [Homebrew](https://brew.sh/). Then:

```sh
xcodebuild -downloadComponent MetalToolchain
brew install mpv pkg-config xxhash python
nvm install && nvm use
make build
```

Xcode supplies Make and the compiler. Homebrew supplies media build dependencies;
VSF builds its pinned mpv, libplacebo and ANGLE/Metal privately. The finished app
bundles its dylibs and is ad-hoc signed; it needs no Homebrew or DYLD settings to
launch. Its actual minimum macOS version depends on the libraries installed on
your build machine and is recorded in the assembly receipt.

## Ubuntu 24.04 amd64 setup

The initial Linux Make recipe follows the maintained Ubuntu package assembler;
other distributions and ARM64 are not yet covered by this entry point.
Enable `deb-src` alongside `deb` in `/etc/apt/sources.list.d/ubuntu.sources`
(the `Types:` line must contain both), then:

```sh
sudo apt-get update
sudo apt-get install -y build-essential git python3-venv pkg-config patchelf ffmpeg \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  libepoxy-dev libavcodec-dev libavdevice-dev libavfilter-dev libavformat-dev libavutil-dev \
  libswresample-dev libswscale-dev libass-dev liblua5.2-dev liblcms2-dev libshaderc-dev \
  libvulkan-dev libxxhash-dev libpulse-dev libasound2-dev libegl1-mesa-dev libgl1-mesa-dev
nvm install && nvm use
make build
make install
```

The assembler downloads matching distribution source packages and bundles media
libraries privately. GTK/WebKit and display/audio drivers remain OS dependencies.
Run the installed app from a graphical desktop with a working desktop portal.

## Windows x64 setup

Install native Windows Python, Git, Rustup using the MSVC host, the pinned Node/npm,
WebView2, Visual Studio Build Tools with **Desktop development with C++** and a
Windows SDK, and [MSYS2](https://www.msys2.org/).
In its CLANG64 shell, update MSYS2 using its documented update procedure and install:

```sh
pacman -S --needed git make diffutils patch curl tar lzip \
  mingw-w64-clang-x86_64-clang mingw-w64-clang-x86_64-pkgconf \
  mingw-w64-clang-x86_64-nasm mingw-w64-clang-x86_64-libplacebo \
  mingw-w64-clang-x86_64-libass mingw-w64-clang-x86_64-luajit \
  mingw-w64-clang-x86_64-libjpeg-turbo mingw-w64-clang-x86_64-lcms2 \
  mingw-w64-clang-x86_64-dav1d mingw-w64-clang-x86_64-opus \
  mingw-w64-clang-x86_64-libvpx mingw-w64-clang-x86_64-libx264 \
  mingw-w64-clang-x86_64-zlib mingw-w64-clang-x86_64-bzip2 mingw-w64-clang-x86_64-xz
```

Open the **x64 Native Tools Command Prompt for VS**, then start CLANG64 while
inheriting its MSVC environment (adjust the MSYS2 location if needed):

```bat
set MSYS2_PATH_TYPE=inherit
C:\msys64\msys2_shell.cmd -defterm -here -no-start -clang64
```

In that shell, enter the repository and run `make build PYTHON=python` or
`make install PYTHON=python`. Native Windows Python, Node/npm, Rust, `cl.exe` and
`lib.exe` must be on PATH. The recipe pins the MSVC linker explicitly so the
MSYS `link` utility cannot be selected accidentally. FFmpeg/mpv use CLANG64;
Rust uses MSVC and an import library generated from the exact mpv DLL being bundled.

## Rebuilds and diagnostics

Run the same Make command after editing. Rust rebuilds incrementally. Successful
native SDK stages are reused under `target/native/<platform>/<recipe-key>`;
recipe, compiler and installed-package changes select a new cache. A completion
marker is written only after the stage succeeds. Do not edit cached SDK files;
remove the affected cache directory to rebuild a damaged cache.

One build per platform is admitted at a time, including separate Make processes.
Failed stages keep their logs and are moved aside on retry. Failed packaging
leaves the last completed app intact. A successful replacement retains the previous
package next to it, as `package.previous-*`; you can delete old packages/caches
when no build or app is using them. After a forced kill, check that all build
processes stopped before removing `target/native/<platform>/build.lock`.

`make build JOBS=3` limits native compilation parallelism (1–16 jobs). `make -j` does not run
multiple app profiles concurrently. Debug and release app outputs are separate;
both use the same tested native SDKs. Debug enables Rust symbols and Tauri devtools,
not the desktop-test instrumentation or hot reload.

For SDK diagnostics and individual recipes, see the [native maintenance contract](native-runtime.md)
and [manual macOS recipes](build-macos-manual.md). Direct Cargo/Tauri commands
without native features remain available for media-free editor tests; they cannot
open media projects.
