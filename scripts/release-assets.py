"""Verify package manifests and assemble the downloadable assets for one release."""
import argparse
import hashlib
from pathlib import Path
import re
import zipfile

from release_version import check

# GitHub requires each asset to be smaller than 2 GiB.
PART_SIZE = 1900 * 1024 * 1024


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def verified(directory):
    files = {}
    for line in (directory / 'SHA256SUMS').read_text(encoding='utf-8-sig').splitlines():
        expected, raw = line.split(None, 1)
        name = raw.removeprefix('*').removeprefix('./')
        path = directory / name
        if ('\\' in name or not path.resolve(strict=True).is_relative_to(directory.resolve())
                or name in files or digest(path) != expected.lower()):
            raise ValueError(f'Invalid checksum entry in {directory.name}: {name}')
        files[name] = path
    return files


def collect(inputs, output, signed):
    version = check()
    output.mkdir(parents=True, exist_ok=False)
    windows = verified(inputs / 'windows-preview-x64')
    setup, = [path for name, path in windows.items() if name.endswith('-setup.exe')]
    assets = [setup, windows[f'VisualSubForge_{version}_x64_windows.zip']]
    inventories = {'windows-x64': windows}
    for deb_arch, arch in [('amd64', 'x86_64'), ('arm64', 'aarch64')]:
        linux = verified(inputs / f'linux-preview-{deb_arch}')
        inventories[f'linux-{deb_arch}'] = linux
        assets.extend([linux[f'VisualSubForge_{version}_{deb_arch}.deb'],
                       linux[f'VisualSubForge_{version}_{arch}_linux.tar.gz']])
    for arch in ['aarch64', 'x86_64']:
        unsigned = verified(inputs / f'macos-unsigned-{arch}')
        inventories[f'macos-{arch}'] = unsigned
        if signed:
            mac = verified(inputs / f'macos-notarized-{arch}')
            assets.append(mac[f'VisualSubForge_{version}_{arch}_macos.dmg'])
        else:
            assets.append(unsigned[f'VisualSubForge_{version}_{arch}_macos-unsigned.dmg'])
    for source in assets:
        source.rename(output / source.name)
    for target, inventory in inventories.items():
        for name in ['native-sources.tar.gz', 'application-sources.tar.gz']:
            path = inventory[name]
            dest = output / f'VisualSubForge_{version}_{target}_{name}'
            if path.stat().st_size <= PART_SIZE:
                path.rename(dest)
                continue
            # Only source archives can be split. Hashes cover the actual parts.
            with path.open('rb') as stream:
                for index in range((path.stat().st_size + PART_SIZE - 1) // PART_SIZE):
                    with dest.with_name(f'{dest.name}.part{index + 1:03}').open('xb') as part:
                        remaining = PART_SIZE
                        while remaining and (chunk := stream.read(min(remaining, 1024 * 1024))):
                            part.write(chunk)
                            remaining -= len(chunk)
            path.unlink()
    fixtures = inputs / 'preview-acceptance-fixtures'
    files = verified(fixtures)
    with zipfile.ZipFile(output / f'VisualSubForge-{version}-test-media.zip', 'x',
                         compression=zipfile.ZIP_DEFLATED) as archive:
        for name in ['SHA256SUMS', *sorted(files)]:
            archive.write(fixtures / name, name)
    manifest = []
    for path in sorted(output.iterdir()):
        if not re.fullmatch(r'[A-Za-z0-9_.+-]+', path.name) or not 0 < path.stat().st_size < 2**31:
            raise ValueError(f'Invalid release asset: {path.name}')
        manifest.append(f'{digest(path)}  {path.name}\n')
        print(f'{path.stat().st_size:12d}  {manifest[-1]}', end='')
    (output / 'SHA256SUMS').write_text(''.join(manifest))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inputs', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--signed', action='store_true')
    args = parser.parse_args()
    collect(args.inputs, args.output, args.signed)
