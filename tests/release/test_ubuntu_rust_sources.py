"""Native distro Rust sources must identify the actual compiled binary inputs."""
import hashlib
import io
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from ubuntu_rust_sources import bind_binary, checked_file, checksums, deb822, rust_inputs, verify_identity


class UbuntuRustSources(unittest.TestCase):
    def test_patched_debian_versions_and_epochs_come_from_buildinfo(self):
        text = ('Source: rust-example\nVersion: 1.0-2ubuntu1\nInstalled-Build-Depends:\n'
                ' libc6 (= 2.39-1),\n librust-example-dev (= 1:2.0-3ubuntu1),\n'
                ' libstd-rust-dev:amd64 (= 1.75.0+dfsg-1),\n rustc (= 1.75.0+dfsg-1)\n')
        fields = deb822(text)
        verify_identity(fields, 'rust-example', '1.0-2ubuntu1')
        self.assertEqual(rust_inputs(fields), [('librust-example-dev', '1:2.0-3ubuntu1'),
            ('libstd-rust-dev', '1.75.0+dfsg-1'), ('rustc', '1.75.0+dfsg-1')])
        with self.assertRaisesRegex(ValueError, 'source version differs'):
            verify_identity(fields, 'rust-example', '1.0')
        fields['Installed-Build-Depends'] = 'librust-example-dev (= 2.0-3)'
        with self.assertRaisesRegex(ValueError, 'lacks exact'):
            rust_inputs(fields)

    def test_signed_descriptor_fields_and_unsafe_checksums(self):
        fields = deb822('-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA512\n\n'
                        'Source: rust-example\nVersion: 1.0-1\nChecksums-Sha256:\n '
                        + 'a' * 64 + ' 42 example.orig.tar.gz\n-----BEGIN PGP SIGNATURE-----\nignored')
        self.assertEqual(checksums(fields), {'example.orig.tar.gz': {'sha256': 'a' * 64, 'size': 42}})
        fields['Checksums-Sha256'] = 'a' * 64 + ' 42 ../escape.tar.gz'
        with self.assertRaisesRegex(ValueError, 'Invalid source checksum'):
            checksums(fields)

    def test_changed_payload_cannot_satisfy_the_source_descriptor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / 'original.tar.gz'
            original.write_bytes(b'wrong source')
            with self.assertRaisesRegex(ValueError, 'Source checksum mismatch'):
                checked_file(original.as_uri(), root / 'download', {'sha256': 'a' * 64, 'size': 12})

    def test_shipped_native_bytes_must_match_the_original_binary_package(self):
        payload = b'compiled native library'
        data = io.BytesIO()
        with tarfile.open(fileobj=data, mode='w:xz') as archive:
            member = tarfile.TarInfo('./usr/lib/x86_64-linux-gnu/libexample.so.0')
            member.size = len(payload)
            archive.addfile(member, io.BytesIO(payload))
        record = {'source': '/usr/lib/x86_64-linux-gnu/libexample.so.0', 'path': 'lib/libexample.so.0',
                  'source_sha256': hashlib.sha256(payload).hexdigest()}
        with patch('ubuntu_rust_sources.subprocess.check_output',
                   side_effect=['debian-binary\ncontrol.tar.xz\ndata.tar.xz\n', data.getvalue()]):
            self.assertEqual(bind_binary(Path('original.deb'), [record])[0]['sha256'], record['source_sha256'])
        record['source_sha256'] = 'a' * 64
        with patch('ubuntu_rust_sources.subprocess.check_output',
                   side_effect=['data.tar.xz\n', data.getvalue()]):
            with self.assertRaisesRegex(ValueError, 'differs from Ubuntu build'):
                bind_binary(Path('original.deb'), [record])


if __name__ == '__main__':
    unittest.main()
