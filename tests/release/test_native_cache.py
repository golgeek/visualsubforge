"""Cache invalidation and Make reuse, with controlled package/toolchain changes."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('native_cache', ROOT / 'scripts/native-cache.py')
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


class NativeCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in ('scripts', 'build', 'tests/native', 'tests/release/windows-requirements.txt'):
            source, target = ROOT / name, self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, target, ignore=shutil.ignore_patterns('__pycache__'))
            else:
                shutil.copy2(source, target)
        self.versions = {'libass': '1', 'lcms2': '2', 'unrelated-brew-formula': '1'}
        self.xcode = 'Xcode test 1'
        self.sdk = 'macOS SDK test 1'
        self.msys = 'selected MSYS2 packages 1'

    def probe(self, *args):
        if args[0] == 'xcodebuild':
            return self.xcode
        if args[0] == 'xcrun':
            return self.sdk
        if args[0] == 'pacman':
            self.assertEqual(list(args[2:]), cache.FFMPEG_PACKAGES)
            return self.msys
        return 'compiler test 1'

    def select(self, platform='macos'):
        return cache.select(self.root, platform, self.probe,
                            lambda names: {name: self.versions.get(name) for name in names}, {})

    def keys(self, selected):
        return {name: item['key'] for name, item in selected.items()}

    def edit(self, name):
        with (self.root / name).open('a') as stream:
            stream.write('\n# controlled input change\n')

    def test_unrelated_upgrade_packaging_and_windows_recipe_reuse_mac_libraries(self):
        baseline = self.select()
        for scenario in ('unrelated brew upgrade', 'packaging script edit', 'Windows recipe edit'):
            with self.subTest(scenario=scenario):
                if scenario == 'unrelated brew upgrade':
                    self.versions['unrelated-brew-formula'] = '2'
                elif scenario == 'packaging script edit':
                    self.edit('scripts/package-linux.py')
                else:
                    self.edit('scripts/build-ffmpeg-windows.sh')
                selected = self.select()
                self.assertEqual(self.keys(selected), self.keys(baseline))
                # Exercise the real Make graph, not only hash equality. Completed
                # stage fixtures represent existing SDKs; no compiler may run.
                if os.name != 'nt':
                    env = {**os.environ, 'VSF_NATIVE_PLATFORM': 'macos', 'VSF_NATIVE_PROFILE': 'release',
                           'VSF_NATIVE_PYTHON': 'python3', 'VSF_NATIVE_JOBS': '3'}
                    for name, item in selected.items():
                        directory = self.root / item['path']
                        directory.mkdir(parents=True, exist_ok=True)
                        (directory / '.complete').touch()
                        env[f'VSF_{name.upper()}_BUILD'] = item['path']
                    upstream = self.root / (selected['mpv']['path'] + '-upstream')
                    upstream.mkdir(parents=True, exist_ok=True)
                    (upstream / '.complete').touch()
                    # Prerequisite completion precedes mpv completion.
                    (self.root / selected['mpv']['path'] / '.complete').touch()
                    result = subprocess.run(['make', '-q', '-f', 'build/native.mk', 'native'],
                                            cwd=self.root, env=env, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_dependency_and_recipe_changes_invalidate_only_consumers(self):
        baseline = self.keys(self.select())
        self.versions['libass'] = '2'
        selected = self.keys(self.select())
        self.assertEqual([k for k in baseline if baseline[k] != selected[k]], ['mpv'])
        self.versions['lcms2'] = '3'
        selected = self.keys(self.select())
        self.assertEqual({k for k in baseline if baseline[k] != selected[k]}, {'placebo', 'mpv'})
        self.edit('scripts/build-angle-macos.py')
        changed = self.keys(self.select())
        self.assertNotEqual(changed['angle'], selected['angle'])
        self.assertEqual(changed['mpv'], selected['mpv'])
        self.assertEqual(changed['placebo'], selected['placebo'])

    def test_xcode_sdk_and_patches_invalidate_native_builds(self):
        baseline = self.keys(self.select())
        self.xcode += ' changed'
        self.assertTrue(all(value != self.keys(self.select())[name] for name, value in baseline.items()))
        baseline = self.keys(self.select())
        self.sdk += ' changed'
        self.assertTrue(all(value != self.keys(self.select())[name] for name, value in baseline.items()))
        baseline = self.keys(self.select())
        patch, = list((self.root / 'tests/native/mpv-render').glob('0002*.patch'))
        self.edit(patch.relative_to(self.root))
        selected = self.keys(self.select())
        self.assertEqual([k for k in baseline if baseline[k] != selected[k]], ['mpv'])

    def test_pkg_config_tracks_selected_versions_prefixes_and_public_dependencies(self):
        answers = {
            ('--modversion', 'library'): '2.0',
            ('--variable=prefix', 'library'): str(self.root / 'keg-v2'),
            ('--cflags', '--libs', 'library'): '-llibrary',
            ('--print-requires', 'library'): 'dependency >= 1.0',
            ('--modversion', 'dependency'): '1.1',
            ('--variable=prefix', 'dependency'): str(self.root / 'dependency-v1'),
            ('--cflags', '--libs', 'dependency'): '-ldependency',
            ('--print-requires', 'dependency'): '',
            ('--modversion', 'optional'): None,
        }
        with patch.object(cache, 'command', side_effect=lambda tool, *args, **kw: answers[args]):
            inputs = cache.pkg_inputs(['library', 'optional'])
        self.assertEqual(inputs['library']['version'], '2.0')
        self.assertEqual(inputs['library']['prefix'], str((self.root / 'keg-v2').resolve()))
        self.assertEqual(inputs['dependency']['version'], '1.1')
        self.assertIsNone(inputs['optional'])

    def test_windows_ffmpeg_inputs_propagate_to_mpv_and_import_sdk(self):
        baseline = self.keys(self.select('windows'))
        self.msys += ' changed'
        selected = self.keys(self.select('windows'))
        self.assertTrue(all(value != selected[name] for name, value in baseline.items()))
        self.edit('scripts/build-ffmpeg-windows.sh')
        changed = self.keys(self.select('windows'))
        self.assertTrue(all(value != changed[name] for name, value in selected.items()))
