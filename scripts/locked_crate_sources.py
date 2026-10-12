"""Preserve registry sources pinned by a native library's embedded Cargo.lock.

The whole lockfile is retained, including optional/build/test dependencies. This
does not claim a per-symbol link map or run Cargo against a moving registry.
"""
import hashlib
import json
import tarfile
import tomllib

from dependency_sources import source_notices, upstream_notices, verified_download


def collect(lock_bytes, output, cached_crates=None):
    output.mkdir(parents=True, exist_ok=False)
    (output / 'Cargo.lock').write_bytes(lock_bytes)
    sources = output / 'sources/rust'
    sources.mkdir(parents=True)
    records, upstream = [], {}
    for entry in tomllib.loads(lock_bytes.decode('utf-8'))['package']:
        name, version = entry['name'], entry['version']
        record = {'name': name, 'version': version}
        if 'source' not in entry:
            record['source'] = 'Included in the parent native source archive'
        else:
            if entry['source'] != 'registry+https://github.com/rust-lang/crates.io-index':
                raise ValueError(f'Unreviewed locked native Rust source: {entry["source"]}')
            filename = f'{name}-{version}.crate'
            archive = sources / filename
            record['source'] = verified_download(
                f'https://static.crates.io/crates/{name}/{filename}', archive, 'sha256',
                bytes.fromhex(entry['checksum']), cached_crates / filename if cached_crates else None)
            with tarfile.open(archive) as contents:
                manifest = tomllib.loads(contents.extractfile(f'{name}-{version}/Cargo.toml').read().decode('utf-8'))['package']
            if manifest['name'] != name or manifest['version'] != version:
                raise ValueError(f'Crate manifest differs from lockfile identity: {name} {version}')
            record['license'] = manifest.get('license')
            record['notices'] = source_notices(archive, output / 'notices/rust', manifest.get('license-file'))
            if not record['notices']:
                record['upstream_notices'] = upstream_notices(manifest, archive, output, upstream)
        records.append(record)
    receipt = {'schema': 1, 'lockfile_sha256': hashlib.sha256(lock_bytes).hexdigest(),
               'packages': records, 'upstream_sources': upstream, 'scope': __doc__.strip()}
    (output / 'locked-crates.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    return receipt
