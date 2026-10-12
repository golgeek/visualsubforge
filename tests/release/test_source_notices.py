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
