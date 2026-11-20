"""Read the workspace release version and check or synchronize packaging manifests."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def release_version(root=ROOT):
    workspace = tomllib.loads((root / 'Cargo.toml').read_text())
    return workspace['workspace']['package']['version']


def check(root=ROOT):
    version = release_version(root)
    package = json.loads((root / 'package.json').read_text())
    lock = json.loads((root / 'package-lock.json').read_text())
    tauri = json.loads((root / 'src-tauri/tauri.conf.json').read_text())
    copies = {'package.json': package['version'], 'package-lock.json': lock['version'],
              'npm root package': lock['packages']['']['version'], 'tauri.conf.json': tauri['version']}
    for entry in tomllib.loads((root / 'Cargo.lock').read_text())['package']:
        if 'source' not in entry:
            copies[entry['name']] = entry['version']
    for name, value in copies.items():
        if value != version:
            raise ValueError(f'{name} has version {value}, expected {version}')
    return version


def sync():
    version = release_version()
    subprocess.run(['npm', 'version', '--no-git-tag-version', '--allow-same-version', version],
                   cwd=ROOT, check=True)
    path = ROOT / 'src-tauri/tauri.conf.json'
    original = path.read_text()
    path.write_text(re.sub(r'("version":\s*)"[^"]+"', rf'\g<1>"{version}"', original, count=1))
    subprocess.run(['cargo', 'metadata', '--offline', '--format-version=1'], cwd=ROOT,
                   stdout=subprocess.DEVNULL, check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sync', action='store_true', help='propagate Cargo.toml to the other manifests')
    if parser.parse_args().sync:
        sync()
    print(check())
