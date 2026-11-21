#!/bin/sh
# Build the private text and shader libraries on Jammy. Only source indexes
# from Noble are needed; all compilers and system libraries come from Jammy.
set -eu
mkdir "$1"
root=$(cd "$1" && pwd)
jobs=${2:-3}
prefix="$root/sdk"
mkdir -p "$root/source-archives" "$root/source" "$prefix"
cd "$root"
source_package() {
    name=$1 version=$2
    mkdir "source-archives/$name" "source/$name"
    (cd "source-archives/$name" && apt-get source --download-only --only-source "$name=$version")
    # apt authenticates the source package against the distro source index.
    tar -xf "source-archives/$name/$name"_*.orig.tar.gz --strip-components=1 -C "source/$name"
}
source_package shaderc 2023.8-1build1
source_package glslang 14.0.0-2
source_package spirv-tools '2023.6~rc1-2'
source_package spirv-headers '1.6.1+1.3.275.0+git20240228-1'
source_package spirv-cross '2021.01.15+1.3.239.0-1build1'
source_package libunibreak 5.1-2build1
source_package libass '1:0.17.1-2build1'
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
for dep in libunibreak libass; do
    (cd "source/$dep"
     if [ ! -f configure ]; then autoreconf -fi; fi
     ./configure --prefix="$prefix" --libdir="$prefix/lib" --disable-static --enable-shared
     make -j "$jobs"
     make install)
done
third="$root/source/shaderc/third_party"
for dep in spirv-tools spirv-headers glslang; do
    rmdir "$third/$dep" 2>/dev/null || test ! -e "$third/$dep"
    ln -s "$root/source/$dep" "$third/$dep"
done
cmake -G Ninja -S source/shaderc -B build-shaderc -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$prefix" -DCMAKE_INSTALL_LIBDIR=lib \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DSHADERC_SKIP_TESTS=ON \
    -DSHADERC_SKIP_EXAMPLES=ON -DSHADERC_SKIP_COPYRIGHT_CHECK=ON \
    -DCMAKE_SHARED_LINKER_FLAGS=-Wl,--exclude-libs,ALL
cmake --build build-shaderc --parallel "$jobs"
cmake --install build-shaderc
cmake -G Ninja -S source/spirv-cross -B build-cross -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$prefix" -DCMAKE_INSTALL_LIBDIR=lib \
    -DSPIRV_CROSS_SHARED=ON -DSPIRV_CROSS_STATIC=OFF -DSPIRV_CROSS_CLI=OFF -DSPIRV_CROSS_ENABLE_TESTS=OFF
cmake --build build-cross --parallel "$jobs"
cmake --install build-cross
tar -czf shader-stack-source.tar.gz source-archives
