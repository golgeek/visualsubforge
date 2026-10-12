import hashlib
import io
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from msys_sources import owned_files, source_notices, verify_binary, verify_recipe


def archive(path, files):
    with tarfile.open(path, 'w:gz') as output:
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(content)
            output.addfile(info, io.BytesIO(content))


class MsysSources(unittest.TestCase):
    def test_binary_bytes_and_source_recipe_must_match_build_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / 'binary.pkg.tar.gz'
            dll = root / 'runtime.dll'
            dll.write_bytes(b'native input')
            recipe = b'pkgname=example\n'
            checksum = hashlib.sha256(recipe).hexdigest()
            archive(binary, {'clang64/bin/runtime.dll': dll.read_bytes(),
                             '.PKGINFO': b'pkgname = example\nlicense = MIT\n',
                             '.BUILDINFO': f'pkgbuild_sha256sum = {checksum}\n'.encode()})
            package, build = verify_binary(binary, {'clang64/bin/runtime.dll': dll})
            self.assertEqual(package['license'], ['MIT'])
            source = root / 'source.tar.gz'
            archive(source, {'example/PKGBUILD': recipe})
            verify_recipe(source, build['pkgbuild_sha256sum'][0])
            dll.write_bytes(b'different local binary')
            with self.assertRaisesRegex(ValueError, 'Installed DLL differs'):
                verify_binary(binary, {'clang64/bin/runtime.dll': dll})
            archive(source, {'example/PKGBUILD': b'changed build recipe\n'})
            with self.assertRaisesRegex(ValueError, 'Source recipe differs'):
                verify_recipe(source, checksum)

    def test_unknown_package_owner_is_not_silently_excluded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / 'var/lib/pacman/local/example-1'
            database.mkdir(parents=True)
            (database / 'desc').write_text('%NAME%\nexample\n\n%VERSION%\n1\n')
            (database / 'files').write_text('%FILES%\nclang64/bin/runtime.dll\n')
            found = owned_files(root, {'clang64/bin/runtime.dll': root / 'runtime.dll'})
            self.assertEqual(found['clang64/bin/runtime.dll'][0]['name'], ['example'])
            with self.assertRaisesRegex(ValueError, 'no package owner'):
                owned_files(root, {'clang64/bin/unknown.dll': root / 'unknown.dll'})

    def test_nested_source_notice_is_retained_and_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inner = root / 'inner.tar.gz'
            archive(inner, {'upstream/LICENSE': b'upstream notice\n', 'upstream/code.c': b'source'})
            outer = root / 'source.tar.gz'
            archive(outer, {'package/upstream.tar.gz': inner.read_bytes(), 'package/PKGBUILD': b'recipe'})
            notices = source_notices(outer, root / 'notices')
            self.assertEqual(len(notices), 1)
            self.assertEqual((root / 'notices' / notices[0]['path']).read_bytes(), b'upstream notice\n')
            archive(outer, {'../LICENSE': b'unsafe'})
            with self.assertRaisesRegex(ValueError, 'Unsafe source notice'):
                source_notices(outer, root / 'rejected')
