"""Choose published MSYS2 archives after verifying every signed build input."""
from collections import defaultdict
import re
import shlex

from native_evidence import digest

PREFIX = 'mingw-w64-clang-x86_64-'
# Unknown packages and changed license declarations always retain full sources.
PERMISSIVE = {
    'compiler-rt': {'spdx:Apache-2.0 WITH LLVM-exception'},
    'libunwind': {'spdx:Apache-2.0 WITH LLVM-exception'},
    'libc++': {'spdx:Apache-2.0 WITH LLVM-exception'},
    'rust': {'spdx:Apache-2.0 AND MIT'},
    'crt': {'spdx:ZPL-2.1'},
    'winpthreads': {'spdx:MIT AND BSD-3-Clause', 'spdx:MIT AND BSD-3-Clause-Clear'},
}


def mingw_snapshot(recipe):
    """Read the checksum for the pinned upstream source; never execute PKGBUILD."""
    commit = re.search(r"""(?m)^_commit=['"]?([0-9a-f]{40})['"]?$""", recipe)
    sources = re.search(r'(?ms)^source=\((.*?)\)', recipe)
    sums = re.search(r'(?ms)^sha256sums=\((.*?)\)', recipe)
    if not (commit and sources and sums):
        return None
    try:
        items = shlex.split(sources[1], comments=True)
        hashes = shlex.split(sums[1], comments=True)
    except ValueError:
        return None
    if len(items) != len(hashes):
        return None
    for item, checksum in zip(items, hashes):
        if (re.fullmatch(r'mingw-w64::git\+https://git\.code\.sf\.net/p/mingw-w64/mingw-w64'
                         r'#commit=\$(?:_commit|\{_commit\})', item)
                and re.fullmatch(r'[a-f0-9]{64}', checksum)):
            return {'commit': commit[1], 'sha256': checksum}
    return None


def retention(packages, sources):
    owners = defaultdict(list)
    headers = {}
    for package in packages:
        name = package['source']
        owners[name].append(package)
        snapshot = sources[name].get('mingw_snapshot')
        if package['name'] == PREFIX + 'headers' and snapshot:
            headers[(snapshot['commit'], snapshot['sha256'])] = name
    decisions = {}
    for name, records in owners.items():
        reason = 'verified permissive toolchain input'
        keep = False
        for package in records:
            short = package['name'].removeprefix(PREFIX)
            if ('shipped DLL' in package['required_by'] or len(package['license']) != 1
                    or package['license'][0] not in PERMISSIVE.get(short, set())):
                keep, reason = True, 'shipped library, source shared with it, or input outside the permissive toolchain list'
                break
            if short in ('crt', 'winpthreads'):
                snapshot = sources[name].get('mingw_snapshot') or {}
                header = headers.get((snapshot.get('commit'), snapshot.get('sha256')))
                if header is None:
                    keep, reason = True, 'no retained headers recipe with the same upstream commit and SHA-256'
                    break
                reason = f'upstream snapshot also in {header}'
        decisions[name] = {'included': keep, 'reason': reason}
    return decisions


def prepare_release(output, packages, sources):
    decisions = retention(packages, sources)
    source_bytes = retained_bytes = binary_bytes = 0
    for name, record in sources.items():
        path = output / 'source-packages' / name
        size = path.stat().st_size
        source_bytes += size
        record.update(decisions[name], bytes=size)
        if record['included']:
            retained_bytes += size
        else:
            path.unlink()
    for package in packages:
        binary = package['binary']
        path = output / 'binary-packages' / binary['file']
        binary['bytes'] = path.stat().st_size
        binary_bytes += binary['bytes']
        path.unlink()
    audit_sources(output / 'source-packages', packages, sources)
    return {'verified_binary_bytes': binary_bytes, 'verified_source_bytes': source_bytes,
            'published_source_bytes': retained_bytes}


def audit_sources(directory, packages, sources):
    for name, rule in retention(packages, sources).items():
        path = directory / name
        if rule['included'] and (not path.is_file() or digest(path) != sources[name]['sha256']):
            raise ValueError(f'Missing or changed required MSYS2 source archive: {name}')
