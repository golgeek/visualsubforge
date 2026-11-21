#!/bin/sh
# Same libdovi source archive as the Windows package. Keep its locked crate
# sources alongside the C API library for offline rebuilding and notices.
set -eu
mkdir "$1"
root=$(cd "$1" && pwd)
cd "$root"
curl --fail --location --retry 3 https://github.com/quietvoid/dovi_tool/archive/libdovi-3.4.0.tar.gz -o upstream.tar.gz
printf '%s  upstream.tar.gz\n' 8eac4d1c3134f53e8eb216db6450307a737425844113e480d1e9713c142a9fa2 | sha256sum -c -
mkdir source
tar -xf upstream.tar.gz --strip-components=1 -C source
cd source/dolby_vision
mkdir -p .cargo
cargo vendor --locked vendor > .cargo/config.toml
cargo cinstall --release --frozen --all-features --prefix="$root/sdk" --libdir="$root/sdk/lib" --library-type=cdylib
cd "$root/source"
tar --exclude='./dolby_vision/target' -czf "$root/source.tar.gz" .
