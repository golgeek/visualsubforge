import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from source_notices import source_notices

LZIP = os.environ.get('VSF_LZIP') or shutil.which('lzip')


class NonArchiveFixtures(unittest.TestCase):
    def test_reviewed_snapshot_bytes_are_retained_and_reported(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'source.tar.gz'
            names = [f'rust/vendor/insta-{v}/tests/snapshots/test_binary__multipart_extension.snap.tar.gz'
                     for v in ('1.43.0', '1.47.2')]
            with tarfile.open(archive, 'w:gz') as source:
                for name, data in [('rust/LICENSE', b'Original license'), *[(n, b'test') for n in names]]:
                    member = tarfile.TarInfo(name)
                    member.size = len(data)
                    source.addfile(member, io.BytesIO(data))
            original = archive.read_bytes()
            fixtures = []
            notices = source_notices(archive, root / 'notices', non_archive_fixtures=fixtures)
            self.assertEqual([n['path'] for n in notices], ['rust/LICENSE'])
            self.assertEqual([f['path'] for f in fixtures], names)
            self.assertTrue(all(f['sha256'] == hashlib.sha256(b'test').hexdigest() for f in fixtures))
            self.assertEqual(archive.read_bytes(), original)

    def test_changed_or_unknown_nested_payload_still_fails(self):
        path = 'rust/vendor/insta-1.43.0/tests/snapshots/test_binary__multipart_extension.snap.tar.gz'
        for name, data, message in [
            (path, b'changed', 'Reviewed non-archive fixture changed'),
            (path.replace('1.43.0', '9.0.0'), b'test', 'Cannot read nested source archive'),
            ('rust/dependency.tar.gz', b'test', 'Cannot read nested source archive'),
        ]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                archive = root / 'source.tar.gz'
                with tarfile.open(archive, 'w:gz') as source:
                    member = tarfile.TarInfo(name)
                    member.size = len(data)
                    source.addfile(member, io.BytesIO(data))
                with self.assertRaisesRegex(ValueError, message):
                    source_notices(archive, root / 'notices')


@unittest.skipUnless(LZIP, 'lzip is required; Windows packaging runs these after MSYS2 setup')
class LzipNotices(unittest.TestCase):
    def test_nested_notice_bytes_and_compression_integrity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            notice = b'Example upstream license\n'
            inner = root / 'upstream.tar'
            with tarfile.open(inner, 'w') as archive:
                member = tarfile.TarInfo('upstream/COPYING')
                member.size = len(notice)
                archive.addfile(member, io.BytesIO(notice))
            payload = subprocess.check_output([LZIP, '--stdout', str(inner)])
            outer = root / 'package.tar.gz'
            for damaged in [False, True]:
                with self.subTest(damaged=damaged):
                    data = payload[:-1] if damaged else payload
                    with tarfile.open(outer, 'w:gz') as archive:
                        member = tarfile.TarInfo('package/upstream.tar.lz')
                        member.size = len(data)
                        archive.addfile(member, io.BytesIO(data))
                    destination = root / str(damaged)
                    if damaged:
                        with self.assertRaisesRegex(ValueError, 'Cannot decode lzip source'):
                            source_notices(outer, destination, lzip=LZIP)
                        self.assertFalse(any(path.is_file() for path in destination.rglob('*')))
                    else:
                        records = source_notices(outer, destination, lzip=LZIP)
                        self.assertEqual(len(records), 1)
                        self.assertEqual(records[0]['sha256'], hashlib.sha256(notice).hexdigest())
                        self.assertEqual((destination / records[0]['path']).read_bytes(), notice)
