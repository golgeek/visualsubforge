#!/usr/bin/env python3
"""Bind Windows native sources and notices to the assembled package before NSIS."""
import argparse
import json
from pathlib import Path
import shutil
import tarfile

from msys_sources import collect
from source_notices import source_notices
from native_evidence import digest

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['package', 'msys', 'mpv', 'ffmpeg', 'platform']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    package, msys, mpv, ffmpeg = (getattr(args, name).resolve(strict=True)
                                for name in ['package', 'msys', 'mpv', 'ffmpeg'])
    assembly = json.loads((package / 'assembly-receipt.json').read_text())
    bundle = package / 'VisualSubForge'
    output = package / 'native-sources'
    output.mkdir(exist_ok=False)
    groups, bindings = {}, []
    for file in assembly['files']:
        original = Path(file['source']).resolve(strict=True)
        if digest(original) != file['sha256'] or digest(bundle / file['path']) != file['sha256']:
            raise ValueError(f'Native file changed since assembly: {file["path"]}')
        if original.is_relative_to(msys):
            origin = 'msys2'
            groups[original.relative_to(msys).as_posix().lower()] = original
        elif original.is_relative_to(ffmpeg / 'sdk'):
            origin = 'ffmpeg'
        elif original.is_relative_to(mpv / 'build'):
            origin = 'mpv'
        elif file['path'] in ['vsf.exe', 'visualsubforge.exe']:
            origin = 'application-inventory.json'
        else:
            raise ValueError(f'Unaccounted native file origin: {original}')
        bindings.append({'path': file['path'], 'sha256': file['sha256'], 'origin': origin})
    inventory = {'schema': 1, 'files': bindings, 'msys2': collect(msys, groups, output / 'msys2')}
    platform = args.platform.resolve(strict=True)
    platform_inventory = json.loads((platform / 'platform-inventory.json').read_text())
    for executable in platform_inventory['executables']:
        if digest(bundle / executable['file']) != executable['sha256']:
            raise ValueError(f'Windows platform evidence identifies a different executable: {executable["file"]}')
    shutil.copytree(platform, output / 'platform')
    inventory['platform'] = {'inventory_sha256': digest(platform / 'platform-inventory.json'),
                             'toolchain': platform_inventory['toolchain'],
                             'review': platform_inventory['review']}
    custom = output / 'custom'
    custom.mkdir()
    shutil.copy2(mpv / 'source.tar.gz', custom / 'mpv-source.tar.gz')
    shutil.copy2(mpv / 'build-receipt.json', custom / 'mpv-build.json')
    shutil.copy2(ffmpeg / 'ffmpeg-8.1.2.tar.xz', custom)
    with tarfile.open(custom / 'ffmpeg-patched-source.tar.gz', 'x:gz') as archive:
        archive.add(ffmpeg / 'source', arcname='source')
    shutil.copytree(ROOT / 'tests/native/ffmpeg-windows', custom / 'ffmpeg-patches')
    shutil.copy2(ROOT / 'scripts/build-ffmpeg-windows.sh', custom)
    for name in ['configure-options.txt', 'build-packages.txt', 'config.h', 'config_components.h',
                 'ffmpeg-version.txt', 'decoders.txt', 'filters.txt']:
        shutil.copy2(ffmpeg / name, custom)
    custom_notices = output / 'custom-notices'
    inventory['custom_notices'] = {}
    for name in ['mpv-source.tar.gz', 'ffmpeg-patched-source.tar.gz']:
        notices = source_notices(custom / name, custom_notices / name)
        if not notices:
            raise ValueError(f'Missing custom source notices: {name}')
        inventory['custom_notices'][name] = notices
    inventory['custom_files'] = {p.relative_to(custom).as_posix(): digest(p)
                                 for p in sorted(custom.rglob('*')) if p.is_file()}
    inventory['scope'] = ('Native file owners, signed MSYS2 sources and custom build inputs; '
                          'static/header-only code and language/platform runtimes need separate review')
    (output / 'native-inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.relative_to(output).as_posix()}\n'
        for p in sorted(output.rglob('*')) if p.is_file()))
    shutil.copy2(output / 'native-inventory.json', bundle)
    shutil.copytree(output / 'msys2/notices', bundle / 'third-party/native/msys2')
    shutil.copytree(custom_notices, bundle / 'third-party/native/custom')
    if (platform / 'notices').is_dir():
        shutil.copytree(platform / 'notices', bundle / 'third-party/native/platform')
    assembly['native_inventory_sha256'] = digest(bundle / 'native-inventory.json')
    for path in [package / 'assembly-receipt.json', bundle / 'native-runtime.json']:
        path.write_text(json.dumps(assembly, indent=2) + '\n')
    # Complete the resource map before Tauri creates the installer. This does
    # not change already-audited native executables or DLLs.
    config_path = package / 'tauri.bundle.json'
    config = json.loads(config_path.read_text())
    config['bundle']['resources'] = {str(path): path.relative_to(bundle).as_posix()
        for path in bundle.rglob('*') if path.is_file() and path.name != 'visualsubforge.exe'}
    config_path.write_text(json.dumps(config, indent=2) + '\n')


if __name__ == '__main__':
    main()
