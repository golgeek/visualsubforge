#!/usr/bin/env bash
# Run in MSYS2 CLANG64. Build outside the checkout; never modify its toolchain.
set -euo pipefail
output=$(cygpath -u "$1")
test ! -e "$output"
mkdir -p "$output/source" "$output/build" "$output/sdk"
archive="$output/ffmpeg-8.1.2.tar.xz"
curl --fail --location --retry 3 https://ffmpeg.org/releases/ffmpeg-8.1.2.tar.xz -o "$archive"
printf '%s  %s\n' 464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c "$archive" | sha256sum -c -
tar -xf "$archive" --strip-components=1 -C "$output/source"
cd "$output/build"
# Built-in codecs/demuxers/filters remain enabled. Explicit external libraries
# cover AV1, VP8/9, Opus, H.264 encoding and compressed inputs. Whisper/GGML is
# deliberately absent: VSF has no speech-recognition operation and that runtime
# has an independently reproduced unload hazard.
options=(
  "--prefix=$(cygpath -m "$output/sdk")" --cc=clang --cxx=clang++
  --disable-autodetect --enable-shared --disable-static --disable-debug
  --disable-doc --disable-ffplay --enable-gpl --enable-version3
  --enable-libdav1d --enable-libopus --enable-libvpx --enable-libx264
  --enable-zlib --enable-bzlib --enable-lzma --enable-schannel
  --enable-dxva2 --enable-d3d11va
)
printf '%s\n' "${options[@]}" > "$output/configure-options.txt"
"$output/source/configure" "${options[@]}" > "$output/configure.log" 2>&1
make -j 3 > "$output/build.log" 2>&1
make install > "$output/install.log" 2>&1
cp config.h config_components.h config.log "$output/"
pacman -Q > "$output/build-packages.txt"
"$output/sdk/bin/ffmpeg.exe" -version > "$output/ffmpeg-version.txt" 2>&1
"$output/sdk/bin/ffmpeg.exe" -decoders > "$output/decoders.txt" 2>&1
"$output/sdk/bin/ffmpeg.exe" -filters > "$output/filters.txt" 2>&1
if grep -Ei 'CONFIG_(LIB)?(WHISPER|GGML).* 1$' config.h config_components.h; then
  echo 'Unexpected speech runtime in FFmpeg configuration' >&2
  exit 1
fi
