import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import urllib.parse

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from homebrew_sources import (HARFBUZZ_TEST_FONT, capture_resource, fetch, matching_recipe, verify_legacy_receipt,
                              verify_source_identity, without_bottle_metadata)
from macos_sources import check_custom_file
from native_evidence import digest, verified_headers
from source_notices import source_notices


class HomebrewSources(unittest.TestCase):
    def test_historical_recipe_uses_core_shard_and_preserves_exact_evidence(self):
        for name, relative in [('libb2', 'Formula/lib/libb2.rb'),
                               ('libpng', 'Formula/lib/libpng.rb'),
                               ('little-cms2', 'Formula/l/little-cms2.rb'),
                               ('openssl@3', 'Formula/o/openssl@3.rb')]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                formula = root / f'{name}.rb'
                formula.write_text('class Example < Formula\nend\n')
                upstream = b'class Example < Formula\n  bottle do\n    revision 1\n  end\n\nend\n'
                sha = 'a' * 40
                with patch('homebrew_sources.subprocess.check_output',
                           side_effect=[json.dumps({'formulae': [{'name': name, 'tap': 'homebrew/core',
                                                                'ruby_source_path': relative}]}),
                                        json.dumps([{'sha': sha}])]) as commits, \
                        patch('homebrew_sources.urllib.request.urlopen',
                              return_value=io.BytesIO(upstream)) as request:
                    record = matching_recipe(formula, root)
                query = urllib.parse.parse_qs(commits.call_args.args[0][2].split('?', 1)[1])
                self.assertEqual(query['path'], [relative])
                url = f'https://raw.githubusercontent.com/Homebrew/homebrew-core/{sha}/{relative}'
                request.assert_called_once_with(url, timeout=60)
                self.assertEqual((record['commit'], record['path'], record['url']), (sha, relative, url))
                evidence = root / 'upstream-formula.rb'
                self.assertEqual(evidence.read_bytes(), upstream)
                self.assertEqual(record['sha256'], digest(evidence))

    def test_lib_recipe_mismatch_does_not_create_provenance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            formula = root / 'libb2.rb'
            formula.write_text('installed recipe')
            with patch('homebrew_sources.subprocess.check_output',
                       side_effect=[json.dumps({'formulae': [{'name': 'libb2', 'tap': 'homebrew/core',
                                                            'ruby_source_path': 'Formula/lib/libb2.rb'}]}),
                                    json.dumps([{'sha': 'a' * 40}])]), \
                    patch('homebrew_sources.urllib.request.urlopen',
                          return_value=io.BytesIO(b'different recipe')):
                with self.assertRaisesRegex(ValueError, 'No matching historical formula for libb2 in 1 revisions'):
                    matching_recipe(formula, root)
            self.assertFalse((root / 'upstream-formula.rb').exists())

    def test_formula_metadata_cannot_redirect_history_outside_core(self):
        for relative in ('../libb2.rb', '/Formula/lib/libb2.rb', 'Formula/lib/other.rb'):
            with self.subTest(relative=relative), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                formula = root / 'libb2.rb'
                formula.write_text('installed recipe')
                metadata = {'formulae': [{'name': 'libb2', 'tap': 'homebrew/core',
                                         'ruby_source_path': relative}]}
                with patch('homebrew_sources.subprocess.check_output', return_value=json.dumps(metadata)) as query:
                    with self.assertRaisesRegex(ValueError, 'Unsafe Homebrew'):
                        matching_recipe(formula, root)
                    self.assertEqual(query.call_count, 1)

    def test_reviewed_raw_font_is_retained_without_treating_it_as_a_tar(self):
        resource = dict(HARFBUZZ_TEST_FONT, name='homebrew-test-ttf')
        def downloaded(url, path, checksum):
            self.assertEqual((url, checksum), (resource['url'], resource['checksum']))
            path.write_bytes(b'font fixture')
            return {'url': url, 'file': path.name, 'sha256': checksum}
        with tempfile.TemporaryDirectory() as directory, \
                patch('homebrew_sources.fetch', side_effect=downloaded), \
                patch('homebrew_sources.source_notices') as notices:
            destination = Path(directory) / 'resource'
            record = capture_resource(resource, destination)
            self.assertEqual((destination / record['file']).read_bytes(), b'font fixture')
            self.assertEqual(record['notices'], [])
            self.assertIn('non-archive', record['kind'])
            notices.assert_not_called()

    def test_changed_font_bytes_are_rejected_and_unknown_fonts_are_not_exempt(self):
        resource = dict(HARFBUZZ_TEST_FONT, name='homebrew-test-ttf')
        with tempfile.TemporaryDirectory() as directory, \
                patch('homebrew_sources.urllib.request.urlopen', return_value=io.BytesIO(b'changed font')):
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                capture_resource(resource, Path(directory) / 'changed')
        with tempfile.TemporaryDirectory() as directory, \
                patch('homebrew_sources.urllib.request.urlopen', return_value=io.BytesIO(b'unknown font')):
            expected = Path(directory) / 'expected'
            expected.write_bytes(b'unknown font')
            resource['checksum'] = digest(expected)
            with self.assertRaises(tarfile.ReadError):
                capture_resource(resource, Path(directory) / 'unknown')

    def test_versioned_source_ids_accept_old_and_escaped_homebrew_formats(self):
        formula = {'name': 'openssl@4', 'stable': {'url': 'https://example.org/source', 'checksum': 'a' * 64}}
        source = {'SPDXID': 'SPDXRef-Archive-openssl.40.4-src', 'downloadLocation': formula['stable']['url'],
                  'checksums': [{'algorithm': 'SHA256', 'checksumValue': 'a' * 64}]}
        legacy = dict(source, SPDXID='SPDXRef-Archive-openssl@4-src')
        for entry in (source, legacy):
            verify_source_identity(formula, {'packages': [entry]})
        with self.assertRaisesRegex(ValueError, 'Missing or ambiguous'):
            verify_source_identity(formula, {'packages': [source, legacy]})
        with self.assertRaisesRegex(ValueError, 'Missing or ambiguous'):
            verify_source_identity(formula, {'packages': [dict(source, SPDXID='SPDXRef-Archive-openssl.40.3-src')]})
        for changed in (dict(source, downloadLocation='https://example.org/other'),
                        dict(source, checksums=[{'algorithm': 'SHA256', 'checksumValue': 'b' * 64}])):
            with self.assertRaisesRegex(ValueError, 'SBOM and formula source differ'):
                verify_source_identity(formula, {'packages': [changed]})
        formula['name'] = 'python@3.14'
        source['SPDXID'] = 'SPDXRef-Archive-python.40.3.2e.14-src'
        verify_source_identity(formula, {'packages': [source]})

    def test_download_restarts_after_partial_connection_reset(self):
        class Interrupted(io.BytesIO):
            def read(self, size=-1):
                if self.tell():
                    raise ConnectionResetError('interrupted')
                return super().read(3)

        with tempfile.TemporaryDirectory() as directory, patch('homebrew_sources.time.sleep'), \
                patch('homebrew_sources.urllib.request.urlopen', side_effect=[
                    Interrupted(b'partial download'), io.BytesIO(b'complete')]) as request:
            path = Path(directory) / 'source'
            expected = Path(directory) / 'expected'
            expected.write_bytes(b'complete')
            fetch('https://example.org/source', path, digest(expected))
            self.assertEqual(path.read_bytes(), b'complete')
            self.assertEqual(request.call_count, 2)

    def test_download_retries_are_bounded_and_preserve_existing_evidence(self):
        with tempfile.TemporaryDirectory() as directory, patch('homebrew_sources.time.sleep'), \
                patch('homebrew_sources.urllib.request.urlopen', side_effect=urllib.error.URLError(
                    ConnectionResetError('reset'))) as request:
            path = Path(directory) / 'source'
            with self.assertRaisesRegex(RuntimeError, 'https://example.org/source'):
                fetch('https://example.org/source', path)
            self.assertEqual(request.call_count, 3)
            self.assertFalse(path.exists())
            path.write_bytes(b'existing evidence')
            with self.assertRaisesRegex(ValueError, 'Refusing to replace'):
                fetch('https://example.org/source', path)
            self.assertEqual(path.read_bytes(), b'existing evidence')
            self.assertEqual(request.call_count, 3)

    def test_permanent_http_error_and_checksum_mismatch_are_not_retried(self):
        with tempfile.TemporaryDirectory() as directory, patch('homebrew_sources.time.sleep'), \
                patch('homebrew_sources.urllib.request.urlopen') as request:
            path = Path(directory) / 'source'
            request.side_effect = urllib.error.HTTPError('https://example.org/source', 404, 'missing', {}, None)
            with self.assertRaisesRegex(RuntimeError, '404'):
                fetch('https://example.org/source', path)
            self.assertEqual(request.call_count, 1)
            self.assertFalse(path.exists())
            request.reset_mock(side_effect=True)
            request.return_value = io.BytesIO(b'wrong bytes')
            with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                fetch('https://example.org/source', path, 'a' * 64)
            self.assertEqual(request.call_count, 1)

    def test_legacy_receipt_cannot_identify_a_different_source_or_local_build(self):
        formula = {'name': 'example', 'stable_version': '1.2.3'}
        receipt = {'poured_from_bottle': True, 'source': {
            'spec': 'stable', 'tap': 'homebrew/core', 'versions': {'stable': '1.2.3'}}}
        verify_legacy_receipt(formula, receipt)
        receipt['source']['versions']['stable'] = '1.2.4'
        with self.assertRaisesRegex(ValueError, 'Unverified legacy'):
            verify_legacy_receipt(formula, receipt)
        receipt['source']['versions']['stable'] = '1.2.3'
        receipt['poured_from_bottle'] = False
        with self.assertRaisesRegex(ValueError, 'Unverified legacy'):
            verify_legacy_receipt(formula, receipt)

    def test_changed_embedded_header_cannot_supply_source_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            header = root / 'xxhash.h'
            header.write_bytes(b'original header')
            receipt = root / 'build.json'
            receipt.write_text(json.dumps({'status': 'passed', 'header_inputs': [
                {'path': str(header), 'sha256': digest(header)}]}))
            self.assertEqual(len(verified_headers(receipt)), 1)
            header.write_bytes(b'replaced after compilation')
            with self.assertRaisesRegex(ValueError, 'Header changed since'):
                verified_headers(receipt)

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
