import hashlib
import io
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from locked_crate_sources import collect
from dependency_sources import upstream_notices, verify_legacy_payload


class LockedNativeCrates(unittest.TestCase):
    def test_legacy_notice_source_must_match_code_and_prebuilt_payloads(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def archive(path, files):
                with tarfile.open(path, 'w:gz') as target:
                    for name, data in files.items():
                        member = tarfile.TarInfo(name)
                        member.size = len(data)
                        target.addfile(member, io.BytesIO(data))
            crate, source = root / 'published.crate', root / 'upstream.tar.gz'
            archive(crate, {'example/Cargo.toml': b'normalized metadata',
                            'example/Cargo.toml.orig': b'original metadata',
                            'example/lib/import.a': b'published import library',
                            'example/build.rs': b'build script'})
            files = {'revision/target/Cargo.toml': b'original metadata',
                     'revision/target/lib/import.a': b'published import library',
                     'revision/target/build.rs': b'build script'}
            archive(source, files)
            self.assertEqual(verify_legacy_payload(crate, source, 'target'), 3)
            files['revision/target/lib/import.a'] = b'a different revision of the library'
            archive(source, files)
            with self.assertRaisesRegex(ValueError, 'payload differs'):
                verify_legacy_payload(crate, source, 'target')
            del files['revision/target/lib/import.a']
            archive(source, files)
            with self.assertRaisesRegex(ValueError, 'payload missing'):
                verify_legacy_payload(crate, source, 'target')
            # The known name/version cannot authorize different published bytes.
            with self.assertRaisesRegex(ValueError, 'No notices or pinned upstream revision'):
                upstream_notices({'name': 'winapi-i686-pc-windows-gnu', 'version': '0.4.0'},
                                 crate, root / 'rejected', {})

    def test_sources_and_notices_are_bound_to_native_lockfile_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            crate = root / 'example-1.0.0.crate'
            with tarfile.open(crate, 'w:gz') as archive:
                for name, data in {'Cargo.toml': b'[package]\nname="example"\nversion="1.0.0"\nlicense="MIT"\n',
                                   'LICENSE': b'The original notice', 'src/lib.rs': b'pub fn example() {}'}.items():
                    member = tarfile.TarInfo('example-1.0.0/' + name)
                    member.size = len(data)
                    archive.addfile(member, io.BytesIO(data))
            checksum = hashlib.sha256(crate.read_bytes()).hexdigest()
            lock = ('version=4\n[[package]]\nname="example"\nversion="1.0.0"\n'
                    'source="registry+https://github.com/rust-lang/crates.io-index"\n'
                    f'checksum="{checksum}"\n').encode()
            record = collect(lock, root / 'captured', root)
            self.assertEqual(record['packages'][0]['license'], 'MIT')
            self.assertEqual(len(record['packages'][0]['notices']), 1)
            crate.write_bytes(b'replaced native dependency source')
            with self.assertRaisesRegex(ValueError, 'Source checksum mismatch'):
                collect(lock, root / 'rejected', root)
