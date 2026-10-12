"""Capture signed MSYS2 packages and corresponding sources for shipped DLLs.

Package recipes are archived as data, never executed. Binary contents must match
the installed compiler inputs, and each source PKGBUILD must match .BUILDINFO.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

from native_evidence import digest
from source_notices import source_notices


def fields(text, database=False):
    result, key = {}, None
    for line in text.splitlines():
        if database:
            if line.startswith('%') and line.endswith('%'):
                key = line.strip('%').lower()
                result[key] = []
            elif line and key:
                result[key].append(line)
        elif ' = ' in line and not line.startswith('#'):
            key, value = line.split(' = ', 1)
            result.setdefault(key.strip(), []).append(value)
    return result


def owned_files(msys, shipped):
    owners = {}
    for path in (msys / 'var/lib/pacman/local').glob('*/desc'):
        metadata = fields(path.read_text(), database=True)
        listed = fields(path.with_name('files').read_text(), database=True).get('files', [])
        for name in listed:
            if name.lower() in shipped:
                if name.lower() in owners:
                    raise ValueError(f'Multiple MSYS2 owners: {name}')
                owners[name.lower()] = (metadata, path.parent)
    if missing := set(shipped) - set(owners):
        raise ValueError(f'Shipped MSYS2 files have no package owner: {sorted(missing)}')
    return owners


def signed_download(url, directory, msys):
    archive = directory / url.rsplit('/', 1)[1]
    signature = Path(str(archive) + '.sig')
    for remote, target in [(url, archive), (url + '.sig', signature)]:
        with urllib.request.urlopen(remote, timeout=120) as response, target.open('xb') as output:
            shutil.copyfileobj(response, output)
    command = [msys / 'usr/bin/bash.exe', '--login', '-c',
               '/usr/bin/pacman-key --verify "$(cygpath -u "$1")" "$(cygpath -u "$2")"',
               'verify-source', str(signature), str(archive)]
    verified = subprocess.run(command, capture_output=True, text=True, check=True)
    Path(str(archive) + '.verification.txt').write_text(verified.stdout + verified.stderr)
    return archive


def verify_binary(archive, installed):
    """Bind package provenance to the bytes actually copied into VSF."""
    with tarfile.open(archive) as source:
        members = {member.name.lower(): member for member in source if member.isfile()}
        for name, path in installed.items():
            with source.extractfile(members[name.lower()]) as stream:
                expected = hashlib.file_digest(stream, 'sha256').hexdigest()
            if digest(path) != expected:
                raise ValueError(f'Installed DLL differs from signed package: {name}')
        package = fields(source.extractfile('.PKGINFO').read().decode())
        build = fields(source.extractfile('.BUILDINFO').read().decode())
    return package, build


def verify_recipe(archive, expected):
    with tarfile.open(archive) as source:
        recipes = [member for member in source if member.isfile() and Path(member.name).name == 'PKGBUILD']
        if len(recipes) != 1:
            raise ValueError(f'Expected one MSYS2 source recipe: {archive}')
        with source.extractfile(recipes[0]) as stream:
            actual = hashlib.file_digest(stream, 'sha256').hexdigest()
        if actual != expected:
            raise ValueError(f'Source recipe differs from binary .BUILDINFO: {archive}')


def collect(msys, shipped, output):
    output.mkdir(parents=True, exist_ok=False)
    binaries, sources, notices = (output / name for name in ['binary-packages', 'source-packages', 'notices'])
    binaries.mkdir()
    sources.mkdir()
    notices.mkdir()
    owners = owned_files(msys, shipped)
    grouped = {}
    for name, (metadata, database) in owners.items():
        package = metadata['name'][0]
        grouped.setdefault(package, {'metadata': metadata, 'database': database, 'files': {}})['files'][name] = shipped[name]
    captured, records = {}, []
    for name, group in sorted(grouped.items()):
        metadata = group['metadata']
        version, arch = metadata['version'][0], metadata['arch'][0]
        filename = f'{name}-{version.split(":")[-1]}-{arch}.pkg.tar.zst'
        url = 'https://repo.msys2.org/mingw/clang64/' + filename
        print(f'Capture signed MSYS2 package: {filename}', flush=True)
        binary = signed_download(url, binaries, msys)
        package, build = verify_binary(binary, group['files'])
        if package['pkgname'] != [name] or package['pkgver'] != [version]:
            raise ValueError(f'Package metadata differs from installation: {name}')
        base = package['pkgbase'][0]
        source_name = f'{base}-{version.split(":")[-1]}.src.tar.zst'
        if source_name not in captured:
            source_url = 'https://repo.msys2.org/mingw/sources/' + source_name
            archive = signed_download(source_url, sources, msys)
            verify_recipe(archive, build['pkgbuild_sha256sum'][0])
            found = source_notices(archive, notices / source_name)
            if not found:
                raise ValueError(f'No source license notices collected: {source_name}')
            captured[source_name] = {'file': source_name, 'url': source_url, 'sha256': digest(archive), 'notices': found}
        else:
            verify_recipe(sources / source_name, build['pkgbuild_sha256sum'][0])
        records.append({'name': name, 'version': version, 'license': package.get('license', []),
                        'binary': {'file': filename, 'url': url, 'sha256': digest(binary)},
                        'source': source_name, 'package_metadata': package, 'build_metadata': build,
                        'shipped_files': {file: digest(path) for file, path in group['files'].items()}})
    receipt = {'packages': records, 'sources': captured,
               'scope': 'Signed package owners of shipped DLLs; static/header-only dependencies need separate review'}
    (output / 'msys-inventory.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt
