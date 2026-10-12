import hashlib
import io
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import subprocess

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from msys_sources import archive_version, embedded_inputs, owned_files, source_notices, verify_binary, verify_recipe, verify_signature


def archive(path, files):
    with tarfile.open(path, 'w:gz') as output:
        for name, content in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(content)
            output.addfile(info, io.BytesIO(content))


class MsysSources(unittest.TestCase):
    def test_signature_failure_retains_the_verifier_diagnostic(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / 'package.pkg.tar.zst'
            result = subprocess.CompletedProcess([], 1, 'Checking package\n', 'gpg: BAD signature\n')
            with patch('msys_sources.subprocess.run', return_value=result):
                with self.assertRaisesRegex(ValueError, 'gpg: BAD signature'):
                    verify_signature(package, root / 'package.sig', root)
            self.assertEqual(Path(str(package) + '.verification.txt').read_text(),
                             'Checking package\ngpg: BAD signature\n')

    def test_embedded_inputs_use_the_binary_build_versions_and_exact_names(self):
        prefix = 'mingw-w64-clang-x86_64-'
        build = {'installed': [prefix + 'crt-14.0.0.r190-1-any', prefix + 'headers-14.0.0.r190-1-any',
                               prefix + 'rust-1~1.87.0-2-any', prefix + 'rust-analyzer-2025.05.01-1-any']}
        selected = embedded_inputs(prefix + 'libdovi', build)
        self.assertIn((prefix + 'rust', '1:1.87.0-2', 'any'), selected)
        self.assertIn((prefix + 'crt', '14.0.0.r190-1', 'any'), selected)
        self.assertEqual(len(selected), 3)
        self.assertEqual(archive_version(selected[0][1]), '14.0.0.r190-1')
        self.assertEqual(archive_version('3:1.4.363.0-1'), '3~1.4.363.0-1')
        with self.assertRaisesRegex(ValueError, 'omits reviewed code inputs'):
            embedded_inputs(prefix + 'libdovi', {'installed': []})

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
