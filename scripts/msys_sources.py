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
from locked_crate_sources import collect as collect_locked_crates

# These can contribute code without appearing in a DLL import graph. Versions
# come from each shipped package's signed .BUILDINFO, never the current mirror.
TOOLCHAIN_INPUTS = {'crt', 'headers', 'compiler-rt', 'libc++', 'libunwind', 'winpthreads'}
LIBRARY_INPUTS = {
    'libplacebo': {'fast_float', 'xxhash', 'python-glad', 'python-jinja', 'vulkan-headers'},
    'libdovi': {'rust'},
    'shaderc': {'glslang', 'spirv-tools', 'spirv-headers'},
    'glslang': {'spirv-tools'},
    'spirv-tools': {'spirv-headers'},
}
PREFIX = 'mingw-w64-clang-x86_64-'


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


def capture_native_rust(archive, base, output):
    if base != 'mingw-w64-libdovi':
        return None
    # This recipe builds dolby_vision/, not the separate dovi_tool CLI in the
    # archive root. Preserve every dependency from the library's own lockfile.
    locks = []
    with tarfile.open(archive) as package:
        for member in package:
            if member.isfile() and member.name.endswith('.tar.gz'):
                with package.extractfile(member) as payload, tarfile.open(fileobj=payload) as source:
                    for entry in source:
                        if entry.isfile() and entry.name.endswith('/dolby_vision/Cargo.lock'):
                            locks.append((entry.name, source.extractfile(entry).read()))
    if len(locks) != 1:
        raise ValueError('Expected the single libdovi library Cargo.lock in its source archive')
    path, contents = locks[0]
    record = collect_locked_crates(contents, output)
    record['lockfile_in_native_source'] = path
    return record


def embedded_inputs(name, build):
    """Read exact package identities for reviewed embedded/generated inputs."""
    requested = TOOLCHAIN_INPUTS | LIBRARY_INPUTS.get(name.removeprefix(PREFIX), set())
    found = []
    for value in build.get('installed', []):
        package, version, release, arch = value.rsplit('-', 3)
        if package.removeprefix(PREFIX) in requested and package.startswith(PREFIX):
            found.append((package, version.replace('~', ':', 1) + '-' + release, arch))
    required = LIBRARY_INPUTS.get(name.removeprefix(PREFIX), set())
    if missing := {PREFIX + suffix for suffix in required} - {item[0] for item in found}:
        raise ValueError(f'Build record omits reviewed code inputs for {name}: {sorted(missing)}')
    return found


def archive_version(version):
    # MSYS2 filenames retain the package epoch, using '~' instead of ':'.
    return version.replace(':', '~')


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
    pending = {}
    for name, group in sorted(grouped.items()):
        metadata = group['metadata']
        version, arch = metadata['version'][0], metadata['arch'][0]
        pending[(name, version, arch)] = {'files': group['files'], 'required_by': ['shipped DLL'], 'embedded': False}
    # Custom FFmpeg/mpv compile with the currently installed MinGW headers/CRT.
    # Retain their actual source packages even if they have no private DLL.
    for path in (msys / 'var/lib/pacman/local').glob('*/desc'):
        metadata = fields(path.read_text(encoding='utf-8'), database=True)
        name = metadata['name'][0]
        if name.removeprefix(PREFIX) not in TOOLCHAIN_INPUTS:
            continue
        key = (name, metadata['version'][0], metadata['arch'][0])
        if key not in pending:
            inputs = {file.lower(): msys / file for file in fields(path.with_name('files').read_text(encoding='utf-8'), database=True)['files']
                      if (file.startswith('clang64/include/') or file.endswith('.a'))
                      and (msys / file).is_file() and not (msys / file).is_symlink()}
            pending[key] = {'files': inputs, 'required_by': ['custom FFmpeg/mpv compiler runtime'], 'embedded': True}
    captured, records, completed = {}, [], set()
    while remaining := [key for key in pending if key not in completed]:
        name, version, arch = key = remaining[0]
        group = pending[key]
        filename = f'{name}-{archive_version(version)}-{arch}.pkg.tar.zst'
        url = 'https://repo.msys2.org/mingw/clang64/' + filename
        print(f'Capture signed MSYS2 package: {filename}', flush=True)
        binary = signed_download(url, binaries, msys)
        package, build = verify_binary(binary, group['files'])
        if package['pkgname'] != [name] or package['pkgver'] != [version] or package['arch'] != [arch]:
            raise ValueError(f'Package metadata differs from installation: {name}')
        base = package['pkgbase'][0]
        source_name = f'{base}-{archive_version(version)}.src.tar.zst'
        if source_name not in captured:
            source_url = 'https://repo.msys2.org/mingw/sources/' + source_name
            archive = signed_download(source_url, sources, msys)
            verify_recipe(archive, build['pkgbuild_sha256sum'][0])
            found = source_notices(archive, notices / source_name)
            if not found:
                raise ValueError(f'No source license notices collected: {source_name}')
            captured[source_name] = {'file': source_name, 'url': source_url, 'sha256': digest(archive), 'notices': found}
            rust_directory = output / 'rust-dependencies' / source_name
            if rust := capture_native_rust(archive, base, rust_directory):
                captured[source_name]['rust_dependencies'] = rust
                shutil.copytree(rust_directory / 'notices', notices / source_name / 'rust-dependencies')
        else:
            verify_recipe(sources / source_name, build['pkgbuild_sha256sum'][0])
        # Do not recursively collect compiler build tools. Only code inputs of
        # the runtime libraries themselves determine this supplementary set.
        if not group['embedded'] or name.removeprefix(PREFIX) in LIBRARY_INPUTS:
            for dependency in embedded_inputs(name, build):
                item = pending.setdefault(dependency, {'files': {}, 'required_by': [], 'embedded': True})
                item['required_by'].append(filename)
        records.append({'name': name, 'version': version, 'license': package.get('license', []),
                        'binary': {'file': filename, 'url': url, 'sha256': digest(binary)},
                        'source': source_name, 'package_metadata': package, 'build_metadata': build,
                        'input_files': {file: digest(path) for file, path in group['files'].items()},
                        'required_by': group['required_by']})
        completed.add(key)
    receipt = {'packages': records, 'sources': captured,
               'scope': 'Signed DLL owners and reviewed compiler/header/generated-code inputs; '
                        'supplemental versions come from each binary build record. Per-recipe coverage still requires review.'}
    (output / 'msys-inventory.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return receipt
