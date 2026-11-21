#!/bin/sh
# Run from the empty build directory. Keep platform policy and the actual
# arguments written to each source receipt together.
set -eu
platform=$1
output=$2
set -- "--prefix=$3"
if [ "$platform" = windows ]; then
    set -- "$@" --cc=clang --cxx=clang++
fi
set -- "$@" --disable-autodetect --enable-shared --disable-static --disable-debug \
    --disable-doc --disable-ffplay --disable-gpl --enable-version3 \
    --enable-libdav1d --enable-zlib --enable-bzlib --enable-lzma
case $platform in
    linux)
        set -- "$@" --enable-vaapi --enable-xlib --disable-outdev=xv \
            "--extra-ldflags=-L$output/bzip2-lib -Wl,--exclude-libs,libdav1d.a:libbz2.a"
        ;;
    macos)
        # VSF opens local media. VideoToolbox supplies auto-copy hardware decode.
        # System AudioToolbox is available for the platform audio codecs.
        xz_prefix=$(pkg-config --variable=prefix liblzma)
        set -- "$@" --enable-videotoolbox --enable-audiotoolbox \
            "--extra-cflags=-I$xz_prefix/include" \
            "--extra-ldflags=-L$xz_prefix/lib -Wl,-headerpad_max_install_names"
        ;;
    windows) set -- "$@" --enable-schannel --enable-dxva2 --enable-d3d11va ;;
    *) echo "Unsupported FFmpeg platform: $platform" >&2; exit 2 ;;
esac
printf '%s\n' "$@" > "$output/configure-options.txt"
"$output/source/configure" "$@" > "$output/configure.log" 2>&1 || { tail -70 "$output/configure.log" >&2; exit 1; }
