"""Build-path transport must preserve exact paths without injecting CI variables."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('native_build_paths', Path(__file__).resolve().parents[2] / 'scripts/native-build-paths.py')
paths = importlib.util.module_from_spec(spec)
spec.loader.exec_module(paths)


class BuildPathTests(unittest.TestCase):
    def test_record_and_export_exact_absolute_paths(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'target/native/macos').mkdir(parents=True)
            old = Path.cwd()
            try:
                os.chdir(root)
                paths.record({'VSF_NATIVE_PLATFORM': 'macos',
                              'VSF_NATIVE_CACHE': 'sdk with spaces', 'CARGO_TARGET_DIR': 'cargo'})
            finally:
                os.chdir(old)
            manifest = root / 'target/native/macos/build-paths.json'
            output = root / 'github-env'
            output.write_text('EXISTING=value\n')
            paths.export(manifest, output)
            self.assertEqual(output.read_text(), 'EXISTING=value\n'
                             f'VSF_NATIVE_CACHE={root / "sdk with spaces"}\n'
                             f'CARGO_TARGET_DIR={root / "cargo"}\n')

    def test_invalid_manifest_cannot_partially_append_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'env'
            manifest = root / 'manifest.json'
            for invalid in ('relative', str(root) + '\nINJECTED=yes', str(root) + '\rX=yes', 1):
                with self.subTest(invalid=invalid):
                    output.write_text('unchanged')
                    manifest.write_text(json.dumps({'VSF_NATIVE_CACHE': str(root),
                                                    'CARGO_TARGET_DIR': invalid}))
                    with self.assertRaises(ValueError):
                        paths.export(manifest, output)
                    self.assertEqual(output.read_text(), 'unchanged')
            manifest.write_text(json.dumps({'UNEXPECTED': str(root)}))
            with self.assertRaises(ValueError):
                paths.export(manifest, output)

    def test_release_compilation_uses_make_without_changing_signing_job(self):
        root = Path(__file__).resolve().parents[2]
        for platform in ('macos', 'linux', 'windows'):
            workflow = (root / '.github/workflows' / f'{platform}-package.yml').read_text()
            self.assertIn('make app JOBS=3 PYTHON=python', workflow)
            self.assertNotIn('npm run tauri -- build', workflow)
            self.assertNotIn('cargo build --locked', workflow)
            self.assertIn('scripts/application-sources.py', workflow)
            self.assertIn(f'scripts/package-{platform}.py', workflow)
            self.assertIn('native-build-paths.py github', workflow)
        mac = (root / '.github/workflows/macos-package.yml').read_text()
        self.assertIn('scripts/sign-macos.py', mac)
        self.assertIn("github.ref == 'refs/heads/main'", mac)
