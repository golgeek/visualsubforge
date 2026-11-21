#!/usr/bin/env python3
"""Select immutable per-library prefixes from explicit build inputs.

Keep the pkg-config probes aligned with the pinned Meson recipes, including
optional dependencies: installing/removing one must invalidate the consumer.
Absent probes are recorded, not replaced by an inventory of the package manager.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess

ROOT = Path(__file__).resolve().parent.parent
AV = 'libavcodec libavfilter libavformat libavutil libswresample libswscale libavdevice'.split()
MPV_COMMON = ('libass subrandr libcurl libcdio libcdio_paranoia dvdread dvdnav iconv mujs '
              'lcms2 libarchive libbluray luajit lua52 lua5.2 lua-5.2 lua51 lua5.1 lua-5.1 lua '
              'rubberband sdl2 uchardet zimg zlib libjpeg openal caca libsixel '
              'shaderc spirv-cross-c-shared vulkan ffnvcodec').split()
MPV_UNIX = ('x11 xext xfixes xpresent xrandr xscrnsaver xv libva libva-x11 '
            'vdpau jack').split()
MPV_LINUX = ('alsa libpulse libpipewire-0.3 sndio libdrm libdisplay-info gbm egl GL '
             'wayland-client wayland-cursor wayland-egl wayland-protocols xkbcommon '
             'libva-drm libva-wayland').split()
PLACEBO_COMMON = 'libunwind lcms2 dovi libxxhash shaderc glslang spirv-cross-c-shared dav1d libavutil libavcodec libavformat'.split()
FFMPEG_PACKAGES = ['bash', 'make', 'diffutils', 'patch', 'tar', 'xz', 'coreutils'] + [
    'mingw-w64-clang-x86_64-' + name for name in
    'clang clang-libs lld llvm-libs compiler-rt libc++ libunwind headers crt winpthreads pkgconf nasm dav1d zlib bzip2 xz'.split()]
COMMON_FILES = ['scripts/native-stage.sh', 'tests/native/build-requirements.txt']
SUPPORT = ['scripts/native_evidence.py', 'scripts/source_archive.py']


def command(*args, optional=False):
    result = subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            env={**os.environ, 'LC_ALL': 'C'})
    if result.returncode:
        if optional:
            return None
        raise RuntimeError(f'{shlex.join(args)}: {result.stderr.strip()}')
    return result.stdout.strip()


def pkg_inputs(names):
    """Version plus selected prefix/flags detects keg revisions and search changes."""
    result = {}
    pending = set(names)
    while pending:
        name = min(pending)
        pending.remove(name)
        if name in result:
            continue
        version = command('pkg-config', '--modversion', name, optional=True)
        if version is None:
            result[name] = None
            continue
        prefix = command('pkg-config', '--variable=prefix', name)
        result[name] = {'version': version,
                        'prefix': str(Path(prefix).resolve()) if prefix else '',
                        'flags': command('pkg-config', '--cflags', '--libs', name)}
        # Public requirements affect the shared library's link interface too.
        for line in command('pkg-config', '--print-requires', name).splitlines():
            pending.add(line.split()[0])
    return result


def file_inputs(root, names):
    result = {}
    for name in names:
        path = root / name
        paths = sorted(path.rglob('*')) if path.is_dir() else [path]
        for item in paths:
            if item.is_file() and '__pycache__' not in item.parts:
                result[item.relative_to(root).as_posix()] = hashlib.sha256(item.read_bytes()).hexdigest()
    return result


def key_for(inputs):
    return hashlib.sha256(json.dumps(inputs, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def select(root, system, probe=command, packages=pkg_inputs, environ=None):
    env = os.environ if environ is None else environ
    host = {'platform': system, 'architecture': platform.machine()}
    if system == 'macos':
        host.update(xcode=probe('xcodebuild', '-version'),
                    sdk=probe('xcrun', '--sdk', 'macosx', '--show-sdk-version'),
                    sdk_build=probe('xcrun', '--sdk', 'macosx', '--show-sdk-build-version'),
                    deployment_target=env.get('MACOSX_DEPLOYMENT_TARGET', '15.0'))
    compiler = 'clang' if system == 'windows' else env.get('CC', 'cc')
    cxx = 'clang++' if system == 'windows' else env.get('CXX', 'c++')
    toolchain = {**host, 'cc_command': compiler, 'cxx_command': cxx,
                 'cc': probe(*shlex.split(compiler), '--version'),
                 'cxx': probe(*shlex.split(cxx), '--version'),
                 'flags': {name: env.get(name, '') for name in ('CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS')}}
    selected = {}

    def add(library, files, machine, dependencies=None):
        inputs = {'schema': 1, 'library': library, 'host': machine,
                  'files': file_inputs(root, files), 'dependencies': dependencies or {}}
        key = key_for(inputs)
        selected[library] = {'key': key, 'path': f'target/native/{system}/{library}/{key}', 'inputs': inputs}
        return key

    if system == 'macos':
        add('angle', ['build/angle.mk', 'scripts/build-angle-macos.py', 'scripts/gclient_sources.py',
                      'tests/native/angle/manifest.json', 'scripts/native-stage.sh', *SUPPORT],
            {**host, 'deployment_target': '14.0'})
    private = {}
    if system == 'macos':
        private['ffmpeg'] = add('ffmpeg', ['build/ffmpeg.mk', 'scripts/configure-ffmpeg.sh',
                                'scripts/build-ffmpeg.sh', 'scripts/macos-ffmpeg-receipt.py', 'scripts/native_evidence.py',
                                *COMMON_FILES], toolchain,
                                {'packages': packages(['liblzma']), 'assembler': probe('nasm', '-v') if platform.machine() == 'x86_64' else None})
    if system == 'linux':
        private['ffmpeg'] = add('ffmpeg', ['build/ffmpeg.mk', 'scripts/configure-ffmpeg.sh',
                                'scripts/build-ffmpeg.sh', *COMMON_FILES], toolchain,
                                {'packages': packages(['libva', 'libva-drm', 'libva-x11', 'x11', 'xext', 'xv', 'zlib', 'liblzma']),
                                 'bzip2': probe('dpkg-query', '-W', '-f=${source:Version}', 'libbz2-dev')})
        private['shaders'] = add('shaders', ['build/linux-shaders.mk', 'build/linux-private-cxx.map', 'scripts/build-linux-shader-stack.sh', 'scripts/native-stage.sh'],
                                 toolchain, packages(['freetype2', 'fribidi', 'fontconfig', 'harfbuzz']))
        private['dovi'] = add('dovi', ['build/ffmpeg.mk', 'scripts/build-linux-dovi.sh',
                              'scripts/native-stage.sh'], toolchain,
                              {'rust': probe('rustc', '-vV'), 'cargo': probe('cargo', '-V')})
    if system != 'windows':
        placebo_names = PLACEBO_COMMON + (['vulkan'] if system == 'linux' else [])
        placebo_inputs = ['build/placebo.mk', 'scripts/build-placebo.py', *COMMON_FILES, *SUPPORT]
        if system == 'linux':
            placebo_inputs.append('build/linux-private-cxx.map')
        dependency_key = add('placebo', placebo_inputs,
                             toolchain, {**packages(placebo_names), **private})
    else:
        dependency_key = add('ffmpeg', ['build/ffmpeg.mk', 'scripts/configure-ffmpeg.sh', 'scripts/build-ffmpeg-windows.sh',
                                        'scripts/native-stage.sh', 'tests/native/ffmpeg-windows'], toolchain,
                             {'msys2': probe('pacman', '-Q', *FFMPEG_PACKAGES)})
    mpv_names = MPV_COMMON + (['libplacebo', 'libva-win32'] if system == 'windows' else AV + MPV_UNIX)
    if system == 'linux':
        mpv_names += MPV_LINUX
    dependencies = {'pkg_config': packages(mpv_names),
                    'ffmpeg' if system == 'windows' else 'placebo': dependency_key}
    mpv_key = add('mpv', ['build/mpv.mk', 'scripts/check-mpv-build.py', 'scripts/prepare-mpv-clock.py',
                          'tests/native/mpv-clock', 'tests/native/mpv-render', 'tests/native/mpv-coreaudio-cleanup',
                          *COMMON_FILES, *SUPPORT], toolchain, dependencies)
    if system == 'windows':
        add('mpv-sdk', ['build/mpv-sdk.mk', 'scripts/prepare-windows-sdk.py',
                        'scripts/native-stage.sh', 'tests/release/windows-requirements.txt'],
            {'msvc': env.get('VCToolsVersion', ''), 'sdk': env.get('WindowsSDKVersion', '')}, {'mpv': mpv_key})
    return selected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', choices=('macos', 'linux', 'windows'), required=True)
    args = parser.parse_args()
    selected = select(ROOT, args.platform)
    destination = ROOT / 'target/native' / args.platform / 'cache-inputs.json'
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(selected, indent=2) + '\n')
    for name, record in selected.items():
        variable = 'VSF_' + name.upper().replace('-', '_')
        print(f'export {variable}_BUILD={shlex.quote(record["path"])}')
        print(f'export {variable}_KEY={record["key"]}')


if __name__ == '__main__':
    main()
