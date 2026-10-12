"""Negative controls for receipt gating, not native playback validation."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipIf(os.name == 'nt', 'Control executables use a POSIX shell')
class PackagedGuiControls(unittest.TestCase):
    def test_zero_exit_alone_and_failure_marker_never_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture = root / 'fixture.mkv'
            fixture.write_bytes(b'control input; not a video')
            executable = root / 'control'
            for index, (body, expected) in enumerate([
                ('exit 0', False),
                ('echo "VSF GPU SMOKE: PASS" >&2; exit 1', False),
                ('echo "VSF GPU SMOKE: FAIL" >&2; echo "VSF GPU SMOKE: PASS" >&2', False),
                # Linux additionally requires observing the packaged mpv in
                # procfs. A shell claiming PASS must fail that independent gate.
                ('echo "VSF GPU SMOKE: PASS" >&2', not sys.platform.startswith('linux')),
            ]):
                executable.write_text('#!/bin/sh\n' + body + '\n')
                executable.chmod(0o755)
                output = root / str(index)
                result = subprocess.run([sys.executable, ROOT / 'tests/release/packaged-gui.py',
                    '--executable', executable, '--fixture', fixture, '--output', output,
                    '--environment', 'receipt control; no native app'], capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, expected, result.stderr)
                receipt = json.loads((output / 'packaged-gui-check.json').read_text())
                self.assertEqual(receipt['status'] == 'passed', expected)
                self.assertEqual(fixture.read_bytes(), b'control input; not a video')
