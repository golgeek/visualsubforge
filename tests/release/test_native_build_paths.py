"""Build-path transport must preserve exact paths without injecting CI variables."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('native_build_paths', Path(__file__).resolve().parents[2] / 'scripts/native-build-paths.py')
paths = importlib.util.module_from_spec(spec)
spec.loader.exec_module(paths)


class BuildPathTests(unittest.TestCase):
    def test_record_and_export_exact_absolute_paths(self):
        for platform, libraries in paths.LIBRARIES.items():
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary).resolve()
                (root / 'target/native' / platform).mkdir(parents=True)
                env = {'VSF_NATIVE_PLATFORM': platform, 'CARGO_TARGET_DIR': 'cargo'}
                for library in libraries:
                    env[f'VSF_{library}_BUILD'] = f'{library} sdk with spaces'
                    env[f'VSF_{library}_KEY'] = 'a' * 64
                old = Path.cwd()
                try:
                    os.chdir(root)
                    paths.record(env)
                finally:
                    os.chdir(old)
                manifest = root / 'target/native' / platform / 'build-paths.json'
                output = root / 'github-env'
                paths.export(manifest, output)
                expected = {name: str(root / value) if name in paths.PATH_FIELDS else value
                            for name, value in env.items() if name != 'VSF_NATIVE_PLATFORM'}
                self.assertEqual(dict(line.split('=', 1) for line in output.read_text().splitlines()), expected)

    def test_invalid_manifest_cannot_partially_append_environment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'env'
            manifest = root / 'manifest.json'
            values = {'CARGO_TARGET_DIR': str(root)}
            for lib in paths.LIBRARIES['macos']:
                values[f'VSF_{lib}_BUILD'] = str(root / lib)
                values[f'VSF_{lib}_KEY'] = 'a' * 64
            for field, invalid in [('CARGO_TARGET_DIR', item) for item in
                                   ('relative', str(root) + '\nINJECTED=yes', str(root) + '\rX=yes', 1)] + [
                                       ('VSF_ANGLE_KEY', 'not-a-key'), ('VSF_ANGLE_KEY', 'a' * 64 + '\nX=y')]:
                with self.subTest(field=field, invalid=invalid):
                    output.write_text('unchanged')
                    manifest.write_text(json.dumps({**values, field: invalid}))
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
        self.assertIn('${{ env.VSF_ANGLE_KEY }}', mac)
        self.assertNotIn("hashFiles('target/native/macos/build-paths.json')", mac)
