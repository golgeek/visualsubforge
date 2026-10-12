"""Capture exact Ubuntu build inputs embedded in bundled native libraries.

Ubuntu builds rav1e using patched librust-* packages, not its upstream Cargo.lock.
Librsvg includes its crate sources in Debian's source payload, but uses a separate
compiler/std build whose source must also be retained.
The original binary's bytes bind the installed library to its Launchpad build;
that buildinfo selects exact source packages, including the compiler/std source.
Rust builds retain all installed Rust inputs (a conservative superset, including
tests/build tools). Shader libraries follow reviewed static/header dependencies
recursively using each dependency's own build record. This is HTTPS publisher provenance plus checksum checks,
not verification of Debian developer signatures or a per-symbol link map.
"""
from concurrent.futures import ThreadPoolExecutor
import gzip
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import urllib.parse
import urllib.request

from dependency_sources import verified_download
from native_evidence import digest
from source_notices import source_notices

API = 'https://api.launchpad.net/1.0/ubuntu'
ARCHIVE = API + '/+archive/primary'
SERIES = API + '/noble'
# Source package names, with exact versions supplied by original buildinfo files.
RUST_ROOTS = {'rust-rav1e', 'librsvg'}
EMBEDDED_INPUTS = {
    'ffmpeg': {'glslang-dev', 'spirv-tools', 'libvulkan-dev'},
    'shaderc': {'glslang-dev', 'spirv-tools', 'spirv-headers'},
    'libplacebo': {'glslang-dev', 'spirv-tools', 'libvulkan-dev', 'libxxhash-dev',
                  'python3-glad', 'python3-jinja2'},
    'glslang': {'spirv-tools', 'spirv-headers'},
    'spirv-tools': {'spirv-headers'},
}
ROOTS = RUST_ROOTS | EMBEDDED_INPUTS.keys()


def deb822(text):
    """Read one Debian control stanza, including clear-signed .dsc payloads."""
    if text.startswith('-----BEGIN PGP SIGNED MESSAGE-----'):
        text = text.split('\n\n', 1)[1].split('\n-----BEGIN PGP SIGNATURE-----', 1)[0]
        text = '\n'.join(line.removeprefix('- ') for line in text.splitlines())
    fields, key = {}, None
    for line in text.splitlines():
        if not line:
            continue
        if line.startswith((' ', '\t')):
            if key is None:
                raise ValueError('Debian control continuation has no field')
            fields[key] += '\n' + line[1:]
        else:
            key, separator, value = line.partition(':')
            if not separator or key in fields:
                raise ValueError(f'Invalid or repeated Debian control field: {key}')
            fields[key] = value.strip()
    return fields


def checksums(fields):
    records = {}
    for line in fields['Checksums-Sha256'].splitlines():
        if not line.strip():
            continue
        checksum, size, filename = line.split()
        if (not re.fullmatch(r'[0-9a-f]{64}', checksum) or not size.isdigit()
                or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.+_~:-]*', filename)
                or filename in records):
            raise ValueError(f'Invalid source checksum entry: {line}')
        records[filename] = {'sha256': checksum, 'size': int(size)}
    if not records:
        raise ValueError('Empty Debian checksum table')
    return records


def installed_inputs(fields):
    result = []
    for entry in fields['Installed-Build-Depends'].split(','):
        match = re.fullmatch(r'\s*([a-z0-9][a-z0-9+.-]*)(?::[a-z0-9-]+)? \(= ([^ )]+)\)\s*', entry)
        if not match:
            raise ValueError(f'Unreviewed build dependency: {entry}')
        name, version = match.groups()
        result.append((name, version))
    return sorted(set(result))


def rust_inputs(fields):
    result = [(name, version) for name, version in installed_inputs(fields)
              if name.startswith('librust-') or name in {'rustc', 'libstd-rust-dev'}]
    if not result or not {'rustc', 'libstd-rust-dev'} <= {name for name, _ in result}:
        raise ValueError('Native Rust build lacks exact crate/compiler/std inputs')
    return sorted(set(result))


def embedded_inputs(source, fields):
    if source in RUST_ROOTS:
        return rust_inputs(fields)
    required = EMBEDDED_INPUTS[source]
    result = [(name, version) for name, version in installed_inputs(fields) if name in required]
    if missing := required - {name for name, _ in result}:
        raise ValueError(f'Ubuntu build omits reviewed embedded inputs for {source}: {sorted(missing)}')
    return result


def fetch(url, path):
    if not url.startswith(('https://api.launchpad.net/', 'https://launchpad.net/')):
        raise ValueError(f'Unexpected Ubuntu publisher URL: {url}')
    with urllib.request.urlopen(url, timeout=120) as response, path.open('xb') as output:
        while block := response.read(1024 * 1024):
            output.write(block)
    return {'url': url, 'file': path.name, 'sha256': digest(path)}


def query(arguments, path):
    url = ARCHIVE + '?' + urllib.parse.urlencode(arguments)
    metadata = fetch(url, path)
    result = json.loads(path.read_text())
    # Exact version queries should have only a few pocket/publication records.
    # Fail visibly rather than silently ignoring an unexpected truncated result.
    if result.get('next_collection_link'):
        raise ValueError(f'Ubuntu exact-version query requires pagination: {url}')
    return result['entries'], metadata


def binary_publications(name, version, path):
    entries, metadata = query({'ws.op': 'getPublishedBinaries', 'binary_name': name,
        'version': version, 'exact_match': 'true', 'distro_arch_series': SERIES + '/amd64'}, path)
    entries = [entry for entry in entries if entry['binary_package_name'] == name
               and entry['binary_package_version'] == version
               and entry['distro_arch_series_link'] == SERIES + '/amd64']
    identities = {(entry['source_package_name'], entry['source_package_version']) for entry in entries}
    if len(identities) != 1:
        raise ValueError(f'Missing/ambiguous Ubuntu source identity: {name} {version}: {identities}')
    return entries, identities.pop(), metadata


def checked_file(url, path, expected):
    record = verified_download(url, path, 'sha256', bytes.fromhex(expected['sha256']))
    if path.stat().st_size != expected['size']:
        raise ValueError(f'Ubuntu source size differs: {path.name}')
    return {**record, 'size': expected['size']}


def verify_identity(fields, name, version):
    if fields.get('Source') != name or fields.get('Version') != version:
        raise ValueError(f'Ubuntu source version differs: expected {name} {version}')


def bind_binary(archive, files):
    """Read, never install, the original .deb to verify pre-relocation bytes."""
    members = subprocess.check_output(['ar', '-t', str(archive)], text=True).splitlines()
    data = [name for name in members if re.fullmatch(r'data\.tar\.(?:xz|gz|zst)', name)]
    if len(data) != 1:
        raise ValueError(f'Expected one Debian binary data archive: {archive}')
    payload = subprocess.check_output(['ar', '-p', str(archive), data[0]])
    bindings = []
    with tarfile.open(fileobj=io.BytesIO(payload)) as contents:
        for record in files:
            original = PurePosixPath(record['source'])
            candidates = [member for member in contents if member.isfile()
                          and PurePosixPath(member.name).as_posix().lstrip('/') == original.as_posix().lstrip('/')]
            if len(candidates) != 1:
                raise ValueError(f'Original Ubuntu binary lacks bundled input: {original}')
            with contents.extractfile(candidates[0]) as stream:
                actual = hashlib.file_digest(stream, 'sha256').hexdigest()
            if actual != record['source_sha256']:
                raise ValueError(f'Bundled native library differs from Ubuntu build: {original}')
            bindings.append({'path': record['path'], 'binary_member': candidates[0].name, 'sha256': actual})
    return bindings


def capture_build(owner, files, output):
    name = owner['package'].split(':')[0]
    publications, identity, publication_record = binary_publications(name, owner['version'], output / 'publication.json')
    if identity != (owner['source'], owner['source_version']):
        raise ValueError(f'Installed native source differs from Ubuntu publication: {name}')
    builds = {entry['build_link'] for entry in publications}
    if len(builds) != 1:
        raise ValueError(f'Ambiguous Ubuntu build: {name}: {builds}')
    build_record = fetch(builds.pop(), output / 'build.json')
    build = json.loads((output / 'build.json').read_text())
    if (build['buildstate'] != 'Successfully built' or build['arch_tag'] != 'amd64'
            or build['distro_series_link'] != SERIES
            or (build['source_package_name'], build['source_package_version']) != identity):
        raise ValueError(f'Unexpected Ubuntu build metadata: {name}')
    changes_record = fetch(build['changesfile_url'], output / 'build.changes')
    changes = deb822((output / 'build.changes').read_text())
    verify_identity(changes, *identity)
    buildinfo_name = Path(urllib.parse.urlparse(build['buildinfo_url']).path).name
    buildinfo_record = checked_file(build['buildinfo_url'], output / 'build.buildinfo', checksums(changes)[buildinfo_name])
    fields = deb822((output / 'build.buildinfo').read_text())
    verify_identity(fields, *identity)
    if fields.get('Build-Architecture') != 'amd64' or fields.get('Build-Origin') != 'Ubuntu':
        raise ValueError(f'Unexpected Ubuntu buildinfo: {name}')
    sums = checksums(fields)
    filename = f'{name}_{owner["version"].split(":")[-1]}_amd64.deb'
    if sums[filename] != checksums(changes)[filename]:
        raise ValueError(f'Ubuntu changes/buildinfo binary mismatch: {filename}')
    binary = output / filename
    binary_record = checked_file(urllib.parse.urljoin(build['buildinfo_url'], filename), binary, sums[filename])
    return {'owner': owner, 'bindings': bind_binary(binary, files), 'publication': publication_record,
            'build': build_record, 'changes': changes_record, 'buildinfo': buildinfo_record,
            'binary': binary_record, 'inputs': embedded_inputs(owner['source'], fields)}


def payload_notices(archive, notices):
    # Debian source format 1.0 stores packaging changes in a compressed diff,
    # including debian/copyright. Keep that complete text alongside upstream
    # notices rather than treating it as a tarball or discarding its notices.
    if archive.name.endswith('.diff.gz'):
        content = gzip.decompress(archive.read_bytes())
        if not content.startswith(b'--- '):
            raise ValueError(f'Invalid Debian source diff: {archive.name}')
        notices.mkdir(parents=True, exist_ok=False)
        path = notices / archive.stem
        path.write_bytes(content)
        return [{'path': path.name, 'sha256': digest(path),
                 'kind': 'Complete Debian source patch, including packaging notices'}]
    return source_notices(archive, notices)


def capture_source(identity, output, notices):
    name, version = identity
    output.mkdir()
    entries, publication = query({'ws.op': 'getPublishedSources', 'source_name': name,
        'version': version, 'exact_match': 'true', 'distro_series': SERIES}, output / 'publication.json')
    entries = [entry for entry in entries if (entry['source_package_name'], entry['source_package_version']) == identity]
    if not entries:
        raise ValueError(f'No exact Ubuntu source publication: {name} {version}')
    # Re-publications share the same source identity; prefer the extant record.
    entries.sort(key=lambda entry: (entry['status'] != 'Published', entry['self_link']))
    url_record = fetch(entries[0]['self_link'] + '?ws.op=sourceFileUrls', output / 'source-urls.json')
    urls = json.loads((output / 'source-urls.json').read_text())
    by_name = {Path(urllib.parse.urlparse(url).path).name: url for url in urls}
    if len(by_name) != len(urls):
        raise ValueError(f'Ambiguous Ubuntu source filenames: {identity}')
    dsc_names = [filename for filename in by_name if filename.endswith('.dsc')]
    if len(dsc_names) != 1:
        raise ValueError(f'Expected one Ubuntu source descriptor: {identity}')
    dsc_name = dsc_names[0]
    descriptor = fetch(by_name[dsc_name], output / dsc_name)
    fields = deb822((output / dsc_name).read_text())
    verify_identity(fields, name, version)
    sums = checksums(fields)
    if set(by_name) != {dsc_name, *sums}:
        raise ValueError(f'Ubuntu source publication differs from descriptor: {identity}')
    archives, notice_records = [], []
    for filename, checksum in sums.items():
        archive = output / filename
        archives.append(checked_file(by_name[filename], archive, checksum))
        notice_records.extend({**item, 'path': (Path(filename) / item['path']).as_posix()}
                              for item in payload_notices(archive, notices / filename))
    if not notice_records:
        raise ValueError(f'No Ubuntu source license notices: {identity}')
    return {'name': name, 'version': version, 'publication': publication, 'urls': url_record,
            'descriptor': descriptor, 'archives': archives, 'notice_files': notice_records}


def collect(files, output, notices):
    output.mkdir(parents=True, exist_ok=False)
    groups = {}
    for record in files:
        owner = record.get('distro', {})
        if owner.get('source') in ROOTS:
            key = (owner['package'].split(':')[0], owner['version'])
            groups.setdefault(key, []).append(record)
    inventory = {'schema': 1, 'scope': __doc__.strip(), 'builds': [], 'inputs': [], 'sources': []}
    required_by, builds = {}, set()

    def capture_input_build(owner, records, reason):
        name, version = owner['package'].split(':')[0], owner['version']
        key = (name, version)
        if key in builds:
            return
        directory = output / (name.replace(':', '_') + '_' + version.replace(':', '~'))
        directory.mkdir()
        build = capture_build(owner, records, directory)
        for dependency in build['inputs']:
            required_by.setdefault(tuple(dependency), []).append(directory.name)
        build['directory'] = directory.name
        build['reason'] = reason
        inventory['builds'].append(build)
        builds.add(key)

    for _, records in sorted(groups.items()):
        capture_input_build(records[0]['distro'], records, 'Bundled shared library')
    metadata = output / 'input-publications'
    metadata.mkdir()

    def resolve(item):
        name, version = item
        path = metadata / (name + '_' + version.replace(':', '~') + '.json')
        _, source, publication = binary_publications(name, version, path)
        return {'name': name, 'version': version, 'source': source, 'publication': publication}

    resolved = set()
    # Most inputs are leaf sources. Follow only reviewed libraries that embed
    # other inputs themselves; do not recursively mirror compiler build tools.
    while pending := sorted(set(required_by) - resolved):
        with ThreadPoolExecutor(max_workers=4) as workers:
            for record in workers.map(resolve, pending):
                inventory['inputs'].append(record)
                resolved.add((record['name'], record['version']))
                if len(resolved) % 50 == 0:
                    print(f'Ubuntu embedded input identities: {len(resolved)}', flush=True)
        for record in inventory['inputs']:
            if record['source'][0] in EMBEDDED_INPUTS:
                owner = {'package': record['name'], 'version': record['version'],
                         'source': record['source'][0], 'source_version': record['source'][1]}
                capture_input_build(owner, [], 'Static/header input selected by a parent buildinfo')
    for record in inventory['inputs']:
        record['required_by'] = sorted(set(required_by[(record['name'], record['version'])]))
    identities = sorted({tuple(record['source']) for record in inventory['inputs']})
    sources = output / 'sources'
    sources.mkdir()

    def capture(identity):
        name = identity[0] + '_' + identity[1].replace(':', '~')
        return capture_source(identity, sources / name, notices / name)

    with ThreadPoolExecutor(max_workers=4) as workers:
        for record in workers.map(capture, identities):
            inventory['sources'].append(record)
            print(f'Ubuntu embedded sources: {len(inventory["sources"])}/{len(identities)}', flush=True)
    inventory['status'] = 'captured'
    inventory['evidence_files_sha256'] = {path.relative_to(output).as_posix(): digest(path)
                                         for path in sorted(output.rglob('*')) if path.is_file()}
    (output / 'inventory.json').write_text(json.dumps(inventory, indent=2) + '\n')
    return inventory
