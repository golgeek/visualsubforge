#!/usr/bin/env bash
# Select toolchains and private native search paths; Make owns the build graph.
set -euo pipefail
make_command=$1
profile=$2
jobs=$3
python=$4
action=$5
cd "$(dirname "$0")/.."
fail() { printf '%s\nSee docs/build.md for prerequisites.\n' "$*" >&2; exit 1; }
[[ "$profile" == release || "$profile" == debug ]] || fail 'Invalid build profile'
[[ "$jobs" =~ ^[1-9][0-9]*$ && "$jobs" -le 16 ]] || fail 'JOBS must be 1–16'
[[ "$PWD" != *[[:space:]]* ]] || fail 'GNU Make native SDK paths require a checkout path without whitespace'
for tool in "$python" node npm rustup cargo git pkg-config; do
    command -v "$tool" >/dev/null || fail "Missing $tool"
done
"$python" -c 'import sys; assert sys.version_info >= (3, 11), "Python 3.11+ required"'
node -e 'const p=require("./package.json"); if(process.version!=="v"+p.engines.node) throw Error("Run nvm install && nvm use: expected Node "+p.engines.node)'
[[ "$(npm --version)" == "$(node -p 'require("./package.json").engines.npm')" ]] || fail 'Wrong npm version; select the pinned Node/npm toolchain'
case "$(uname -s)" in
    Darwin)
        platform=macos
        command -v brew >/dev/null || fail 'Homebrew is required'
        xcodebuild -version >/dev/null
        SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
        export SDKROOT
        export MACOSX_DEPLOYMENT_TARGET=15.0
        # Select only the media inputs we actually link. Never search the
        # global Homebrew pkg-config directory (it may contain another FFmpeg).
        prefix=$(brew --prefix)
        formulae=(libass little-cms2 shaderc xxhash xz)
        packages=$(brew deps --union "${formulae[@]}")
        os_major=$(sw_vers -productVersion | cut -d. -f1)
        PKG_CONFIG_LIBDIR="$(brew --repository)/Library/Homebrew/os/mac/pkgconfig/$os_major"
        for formula in $packages "${formulae[@]}"; do
            for directory in "$prefix/opt/$formula/lib/pkgconfig" "$prefix/opt/$formula/share/pkgconfig"; do
                if [[ -d "$directory" ]]; then PKG_CONFIG_LIBDIR="$directory${PKG_CONFIG_LIBDIR:+:$PKG_CONFIG_LIBDIR}"; fi
            done
        done
        export PKG_CONFIG_LIBDIR PKG_CONFIG_PATH=""
        pkg-config --exists libass lcms2 shaderc libxxhash liblzma || fail 'Missing macOS build prerequisites'
        if pkg-config --exists libavcodec; then fail 'An external FFmpeg is visible in the macOS build search path'; fi
        ;;
    Linux)
        platform=linux
        # shellcheck source=/dev/null
        . /etc/os-release
        case "$ID:$VERSION_ID:$(uname -m)" in
            ubuntu:22.04:x86_64|ubuntu:22.04:aarch64) ;;
            *) fail 'Build the portable Linux package natively in Ubuntu 22.04';;
        esac
        for tool in patchelf nasm; do command -v "$tool" >/dev/null || fail "Missing $tool"; done
        pkg-config --exists gtk+-3.0 epoxy libva || fail 'Missing native development packages'
        ;;
    MINGW*|MSYS*)
        platform=windows
        [[ "${MSYSTEM:-}" == CLANG64 ]] || fail 'Use MSYS2 CLANG64 with the x64 MSVC developer environment inherited'
        command -v lib.exe >/dev/null || fail 'MSVC lib.exe is missing; start MSYS2 from the x64 VS developer shell'
        command -v cl.exe >/dev/null || fail 'MSVC cl.exe is missing'
        [[ "${VSCMD_ARG_TGT_ARCH:-}" == x64 ]] || fail 'Select the x64 Native Tools environment, not x86 or ARM64'
        "$python" -c 'import sys; assert sys.platform == "win32", "Use native Windows Python, not MSYS Python"'
        [[ "$(rustc -vV)" == *'host: x86_64-pc-windows-msvc'* ]] || fail 'Use x86_64-pc-windows-msvc Rust'
        export PYTHONUTF8=1
        export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
        msvc_bin=$(dirname "$(command -v cl.exe)")
        CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="$(cygpath -m "$msvc_bin/link.exe")"
        export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER
        export CC_x86_64_pc_windows_msvc="${CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER%/link.exe}/cl.exe"
        export CXX_x86_64_pc_windows_msvc="$CC_x86_64_pc_windows_msvc"
        ;;
    *) fail 'Unsupported native build platform';;
esac
mkdir -p "target/native/$platform"
lock="target/native/$platform/build.lock"
mkdir "$lock" 2>/dev/null || fail "Another native build owns $lock. If it was killed, verify it stopped before removing that directory."
printf '%s\n' "$$" > "$lock/pid"
trap 'rm -f "$lock/pid"; rmdir "$lock"' EXIT
export VSF_NATIVE_PLATFORM="$platform"
export VSF_NATIVE_PROFILE="$profile"
export VSF_NATIVE_JOBS="$jobs"
venv="target/native/$platform/tools"
if [[ "$platform" == windows ]]; then tools_bin="$venv/Scripts"; else tools_bin="$venv/bin"; fi
if [[ ! -f "$tools_bin/python" && ! -f "$tools_bin/python.exe" ]]; then "$python" -m venv "$venv"; fi
export PATH="$PWD/$tools_bin:$PATH"
export VSF_NATIVE_PYTHON="$tools_bin/python"
"$VSF_NATIVE_PYTHON" -m pip install --only-binary=:all: -r tests/native/build-requirements.txt
if [[ "$platform" == windows ]]; then
    "$VSF_NATIVE_PYTHON" -m pip install --only-binary=:all: --require-hashes -r tests/release/windows-requirements.txt
fi
export CARGO_TARGET_DIR="$PWD/target/native/$platform/app-target"
if [[ "$platform" == windows ]]; then CARGO_TARGET_DIR="$(cygpath -m "$CARGO_TARGET_DIR")"; fi
# Each library gets an immutable prefix from its own recipe/toolchain inputs.
# The selector emits only shell-quoted assignments, never user-supplied code.
cache_assignments=$("$VSF_NATIVE_PYTHON" scripts/native-cache.py --platform "$platform")
eval "$cache_assignments"
# Record paths before compilation so CI can retain failed-stage evidence too.
"$VSF_NATIVE_PYTHON" scripts/native-build-paths.py record
"$make_command" --no-print-directory -f build/native.mk "$action"
