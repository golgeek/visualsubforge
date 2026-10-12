import io
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from homebrew_sources import verify_source_identity, without_bottle_metadata
from macos_sources import check_custom_file
from source_notices import source_notices


class HomebrewSources(unittest.TestCase):
    def test_recipe_matching_ignores_only_bottle_metadata(self):
        recipe = 'class Example < Formula\n  url "https://example.org/v1.tar.gz"\n'
        end = '  def install\n    system "make", "install"\n  end\nend\n'
        upstream = recipe + '\n  bottle do\n    sha256 arm64_sonoma: "abc"\n  end\n' + end
        self.assertEqual(without_bottle_metadata(upstream), recipe + end)
        self.assertNotEqual(without_bottle_metadata(upstream.replace('v1.tar.gz', 'v2.tar.gz')), recipe + end)
        self.assertNotEqual(without_bottle_metadata(upstream.replace('"make"', '"cmake"')), recipe + end)

    def test_source_must_match_the_installed_sbom(self):
        formula = {'name': 'example', 'stable': {'url': 'https://example.org/v1.tar.gz', 'checksum': 'a' * 64}}
        source = {'SPDXID': 'SPDXRef-Archive-example-src', 'downloadLocation': formula['stable']['url'],
                  'checksums': [{'algorithm': 'SHA256', 'checksumValue': 'a' * 64}]}
        verify_source_identity(formula, {'packages': [source]})
        source['checksums'][0]['checksumValue'] = 'b' * 64
        with self.assertRaisesRegex(ValueError, 'SBOM and formula source differ'):
            verify_source_identity(formula, {'packages': [source]})
        with self.assertRaisesRegex(ValueError, 'Missing or ambiguous'):
            verify_source_identity(formula, {'packages': []})

    def test_spdx_license_directory_is_included(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'source.tar.gz'
            with tarfile.open(archive, 'w:gz') as output:
                data = b'The actual SPDX license text\n'
                info = tarfile.TarInfo('glib/LICENSES/LGPL-2.1-or-later.txt')
                info.size = len(data)
                output.addfile(info, io.BytesIO(data))
            records = source_notices(archive, root / 'notices')
            self.assertEqual(len(records), 1)
            self.assertEqual((root / 'notices' / records[0]['path']).read_bytes(), data)

    def test_custom_binary_must_match_build_receipt(self):
        path = Path('/sdk/out/Metal/libEGL.dylib')
        receipt = {'binaries': [{'file': path.name, 'sha256': 'a' * 64}]}
        check_custom_file(path, 'a' * 64, receipt)
        with self.assertRaisesRegex(ValueError, 'differs from its build receipt'):
            check_custom_file(path, 'b' * 64, receipt)
        with self.assertRaisesRegex(ValueError, 'differs from its build receipt'):
            check_custom_file(Path('/sdk/libUnexpected.dylib'), 'a' * 64, receipt)
