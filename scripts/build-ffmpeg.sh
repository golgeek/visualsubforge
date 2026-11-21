#!/bin/sh
# Native POSIX builds. Both libraries share an immutable SDK prefix.
set -eu
recipe=$(cd "$(dirname "$0")/.." && pwd)
jobs=${2:-3}
case $jobs in ''|*[!0-9]*) exit 2 ;; esac
[ "$jobs" -ge 1 ] && [ "$jobs" -le 64 ] || exit 2
mkdir "$1"
output=$(cd "$1" && pwd)
cd "$output"
fetch() {
    curl --fail --location --retry 3 "$1" -o "$2"
    printf '%s  %s\n' "$3" "$2" | shasum -a 256 -c -
}
fetch https://download.videolan.org/pub/videolan/dav1d/1.5.5/dav1d-1.5.5.tar.xz dav1d.tar.xz \
    ca96b1d0ba3f01f55de7231d775892b64994ba061aaeeaaa4f662a390b3abf9e
fetch https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz ffmpeg.tar.xz \
    464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c
mkdir dav1d source build
tar -xf dav1d.tar.xz --strip-components=1 -C dav1d
tar -xf ffmpeg.tar.xz --strip-components=1 -C source
# Arch WebKitGTK loads its distribution dav1d through libavif.
# Give libavcodec a hidden PIC copy with no process-wide dav1d exports.
meson setup dav1d-build dav1d --prefix="$output/sdk" --libdir=lib --buildtype=release \
    -Db_staticpic=true --default-library=static --wrap-mode=nodownload -Denable_tools=false -Denable_tests=false
meson compile -C dav1d-build -j "$jobs"
meson install -C dav1d-build
export PKG_CONFIG_PATH="$output/sdk/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
set -- "--prefix=$output/sdk"
while IFS= read -r option; do set -- "$@" "$option"; done < "$recipe/build/ffmpeg-options.txt"
case $(uname -s) in
    Linux) set -- "$@" --enable-vaapi --extra-ldflags=-Wl,--exclude-libs,libdav1d.a ;;
    *) echo 'Unsupported FFmpeg build platform' >&2; exit 2 ;;
esac
printf '%s\n' "$@" > configure-options.txt
cd build
"$output/source/configure" "$@" > ../configure.log 2>&1 || { tail -70 ../configure.log >&2; exit 1; }
make -j "$jobs" > ../build.log 2>&1 || { tail -70 ../build.log >&2; exit 1; }
make install > ../install.log 2>&1
cp config.h config_components.h ffbuild/config.log ..
readelf -d "$output/sdk/lib/libavcodec.so" > ../avcodec-dynamic.txt
nm -D "$output/sdk/lib/libavcodec.so" > ../avcodec-symbols.txt
if grep -E 'NEEDED.*libdav1d' ../avcodec-dynamic.txt || grep -E ' dav1d_' ../avcodec-symbols.txt; then
    echo 'libavcodec must keep its bundled dav1d private' >&2
    exit 1
fi
grep -qx '#define CONFIG_LIBDAV1D_DECODER 1' config_components.h
LD_LIBRARY_PATH="$output/sdk/lib" "$output/sdk/bin/ffmpeg" -version > ../ffmpeg-version.txt
LD_LIBRARY_PATH="$output/sdk/lib" "$output/sdk/bin/ffmpeg" -decoders > ../decoders.txt
