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


class LockedNativeCrates(unittest.TestCase):
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
