"""Failure/publication controls and native Make profile contracts, without SDKs."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipIf(os.name == 'nt', 'POSIX shell controls; Windows needs an MSYS2 run')
class NativeMakeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.output = self.root / 'app with spaces'

    def run_script(self, name, *arguments, check=True):
        return subprocess.run(['bash', str(ROOT / 'scripts' / name), str(self.output),
                               *arguments], cwd=ROOT, capture_output=True, text=True,
                              check=check)

    def test_stage_failed_attempt_is_preserved_and_retry_completes(self):
        result = self.run_script('native-stage.sh', 'bash', '-c',
                                 'mkdir "$1"; echo broken > "$1/log"; exit 7', '_',
                                 '{output}', check=False)
        self.assertEqual(result.returncode, 7)
        self.assertFalse((self.output / '.complete').exists())
        self.run_script('native-stage.sh', 'mkdir', '{output}')
        self.assertTrue((self.output / '.complete').exists())
        failures = list(self.root.glob('*.failed-*'))
        self.assertEqual(len(failures), 1)
        self.assertEqual((failures[0] / 'log').read_text(), 'broken\n')
        # The successful SDK is reused instead of running a new command.
        self.run_script('native-stage.sh', 'false')

    def test_failed_package_does_not_replace_previous_app(self):
        self.output.mkdir()
        (self.output / 'old').write_text('working')
        result = self.run_script('native-publish.sh', 'bash', '-c',
                                 'mkdir "$1"; echo failure > "$1/log"; exit 8', '_',
                                 '{output}', check=False)
        self.assertEqual(result.returncode, 8)
        self.assertEqual((self.output / 'old').read_text(), 'working')
        self.assertEqual(len(list(self.root.glob('*.pending-*'))), 1)

    def test_successful_package_publishes_and_retains_previous(self):
        self.output.mkdir()
        (self.output / 'old').write_text('working')
        self.run_script('native-publish.sh', 'bash', '-c',
                        'mkdir "$1"; echo native > "$1/new"', '_', '{output}')
        self.assertEqual((self.output / 'new').read_text(), 'native\n')
        self.assertFalse((self.output / 'old').exists())
        previous, = self.root.glob('*.previous-*')
        self.assertEqual((previous / 'old').read_text(), 'working')

    def test_three_publishes_retain_only_immediate_predecessor(self):
        for version in ('one', 'two', 'three'):
            self.run_script('native-publish.sh', 'bash', '-c',
                            'mkdir "$1"; echo "$2" > "$1/version"', '_', '{output}', version)
        previous, = self.root.glob('*.previous-*')
        self.assertEqual((previous / 'version').read_text(), 'two\n')
        self.assertEqual((self.output / 'version').read_text(), 'three\n')

    def test_failed_final_rename_restores_working_package(self):
        self.output.mkdir()
        (self.output / 'version').write_text('working')
        move = self.root / 'mv'
        move.write_text('#!/bin/sh\ncase "$1" in *.pending-*) exit 1;; esac\nexec "$REAL_MV" "$@"\n')
        move.chmod(0o755)
        result = subprocess.run(['bash', str(ROOT / 'scripts/native-publish.sh'), str(self.output),
                                 'mkdir', '{output}'], capture_output=True,
                                env={**os.environ, 'REAL_MV': shutil.which('mv'),
                                     'PATH': str(self.root) + os.pathsep + os.environ['PATH']})
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.output / 'version').read_text(), 'working')
        self.assertFalse(list(self.root.glob('*.previous-*')))

    def test_cleanup_failure_keeps_successful_package(self):
        self.output.mkdir()
        self.run_script('native-publish.sh', 'mkdir', '{output}')
        # Model a locked Windows backup without depending on host permissions.
        deny_remove = self.root / 'rm'
        deny_remove.write_text('#!/bin/sh\nexit 1\n')
        deny_remove.chmod(0o755)
        result = subprocess.run(['bash', str(ROOT / 'scripts/native-publish.sh'), str(self.output),
                                 'mkdir', '-p', '{output}/latest'], check=True,
                                capture_output=True, text=True,
                                env={**os.environ, 'PATH': str(self.root) + os.pathsep + os.environ['PATH']})
        self.assertTrue((self.output / 'latest').is_dir())
        self.assertIn('Could not remove old package:', result.stderr)

    def test_success_without_output_cannot_replace_app(self):
        self.output.mkdir()
        (self.output / 'old').touch()
        self.assertNotEqual(self.run_script('native-publish.sh', 'true', check=False).returncode, 0)
        self.assertTrue((self.output / 'old').exists())

    def test_nested_mac_package_output(self):
        self.run_script('native-publish.sh', 'mkdir', '-p', '{output}/VisualSubForge.app')
        self.assertTrue((self.output / 'VisualSubForge.app').is_dir())

    def test_invalid_job_limits_fail_before_build_or_package_changes(self):
        for jobs in ('0', '17', '-1', 'oops'):
            result = subprocess.run(['bash', str(ROOT / 'scripts/native-env.sh'),
                                     'make', 'release', jobs, 'python3', 'build'],
                                    cwd=ROOT, text=True, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('JOBS must be 1–16', result.stderr)
        result = subprocess.run(['bash', str(ROOT / 'scripts/build-ffmpeg-windows.sh'),
                                 str(self.output), '0'], cwd=ROOT, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.output.exists())

    def test_root_default_and_debug_install_profile(self):
        default = subprocess.check_output(['make', '-n'], cwd=ROOT, text=True)
        self.assertIn('"release"', default)
        self.assertNotIn('build-install', default)
        debug = subprocess.check_output(['make', '-n', 'debug', 'install'], cwd=ROOT, text=True)
        self.assertEqual(debug.count('"debug"'), 2)
        self.assertNotIn('"release"', debug)
        self.assertNotIn('build-install', debug)
        install = subprocess.check_output(['make', '-n', 'install'], cwd=ROOT, text=True)
        self.assertIn('build-install', install)

    def test_each_platform_builds_native_gui_and_matching_cli(self):
        # cygpath is used at Make parse time on Windows; no SDK is needed to
        # inspect that graph on a POSIX development host.
        cygpath = self.root / 'cygpath'
        cygpath.write_text('#!/bin/sh\nprintf "%s\\n" "$2"\n')
        cygpath.chmod(0o755)
        for platform in ('macos', 'linux', 'windows'):
            for profile in ('release', 'debug'):
                with self.subTest(platform=platform, profile=profile):
                    env = {**os.environ, 'PATH': str(self.root) + os.pathsep + os.environ['PATH'],
                           **{f'VSF_{lib}_BUILD': f'target/native/test-no-sdk/{lib}'
                              for lib in ('ANGLE', 'PLACEBO', 'MPV', 'FFMPEG', 'MPV_SDK')},
                           'VSF_NATIVE_PLATFORM': platform, 'VSF_NATIVE_PROFILE': profile,
                           'VSF_NATIVE_PYTHON': 'python3', 'VSF_NATIVE_JOBS': '2',
                           'CARGO_TARGET_DIR': 'target/native/test-target'}
                    plan = subprocess.check_output(['make', '-n', '-f', 'build/native.mk', 'build', 'PYTHON=wrong-interpreter'],
                                                   cwd=ROOT, env=env, text=True)
                    features = 'macos-angle' if platform == 'macos' else 'media-prototype'
                    self.assertIn('--features ' + features + ',bundled-tools', plan)
                    self.assertIn('-p vsf-cli --features bundled-tools', plan)
                    self.assertIn('--mpv-build-receipt', plan)
                    self.assertNotIn('wrong-interpreter', plan)
                    self.assertIn('package-' + platform + '.py', plan)
                    self.assertNotIn('desktop-e2e', plan)
                    if platform == 'windows':
                        self.assertIn('env CC=clang CXX=clang++ python3 scripts/check-mpv-build.py', plan)
                    if profile == 'release':
                        self.assertIn('cargo build --locked --release', plan)
                        self.assertNotIn('build --debug', plan)
                    else:
                        self.assertIn('build --debug', plan)
                        self.assertNotIn('cargo build --locked --release', plan)

    @unittest.skipUnless(shutil.which('ditto'), 'macOS installation uses ditto')
    def test_install_copies_self_contained_app_to_selected_parent(self):
        package = self.root / 'package'
        executable = package / 'VisualSubForge.app/Contents/MacOS/visualsubforge'
        executable.parent.mkdir(parents=True)
        executable.write_text('#!/bin/sh\nexit 0\n')
        executable.chmod(0o755)
        destination = self.root / 'Applications with spaces'
        subprocess.run(['bash', str(ROOT / 'scripts/native-install.sh'), 'macos', str(package)],
                       cwd=ROOT, env={**os.environ, 'INSTALL_DIR': str(destination)},
                       check=True, capture_output=True)
        self.assertEqual((destination / 'VisualSubForge.app/Contents/MacOS/visualsubforge').read_bytes(),
                         executable.read_bytes())
