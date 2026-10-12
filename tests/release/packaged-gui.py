#!/usr/bin/env python3
"""Run the production package's existing native smoke diagnostic and ordered exit.

Checks commands, timing, replacement and native render readiness. This does not
observe displayed pixels, speaker output, interactive dialogs or device latency.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['executable', 'fixture', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--environment', required=True, help='Describe the actual runner/display/audio setup')
    args = parser.parse_args()
    executable = args.executable.resolve(strict=True)
    fixture = args.fixture.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(('VSF_', 'LD_', 'DYLD_', 'PKG_CONFIG_'))}
    env['PATH'] = str(Path(env['SystemRoot']) / 'System32') if os.name == 'nt' else '/usr/bin:/bin'
    env['VSF_MEDIA_SMOKE'] = str(fixture)
    env['VSF_DIAGNOSTIC_EXIT'] = '1'
    evidence = {'status': 'incomplete', 'environment': args.environment,
                'scope': __doc__.strip(), 'executable_sha256': hashlib.sha256(executable.read_bytes()).hexdigest(),
                'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest()}
    process, mapped = None, set()
    try:
        with (args.output / 'stdout.log').open('wb') as stdout, (args.output / 'stderr.log').open('wb') as stderr:
            process = subprocess.Popen([executable], cwd=args.output, env=env, stdout=stdout, stderr=stderr,
                                       start_new_session=os.name != 'nt')
            deadline = time.monotonic() + 90
            while process.poll() is None:
                if time.monotonic() > deadline:
                    raise TimeoutError('Packaged GUI did not complete native smoke and ordered shutdown in 90 seconds')
                maps = Path(f'/proc/{process.pid}/maps')
                try:
                    for line in maps.read_text().splitlines():
                        parts = line.split(None, 5)
                        if len(parts) == 6 and parts[5].startswith('/') and '.so' in parts[5]:
                            mapped.add(parts[5])
                except FileNotFoundError:
                    pass  # Windows/macOS have no procfs; not claimed as coverage.
                time.sleep(0.1)
        evidence['exit'] = process.returncode
        log = (args.output / 'stderr.log').read_text(errors='replace')
        if process.returncode != 0 or 'VSF GPU SMOKE: PASS' not in log or 'VSF GPU SMOKE: FAIL' in log:
            raise ValueError(f'Packaged native smoke failed (exit {process.returncode}); inspect stderr.log')
        if hashlib.sha256(fixture.read_bytes()).hexdigest() != evidence['fixture_sha256']:
            raise ValueError('Native smoke changed its input fixture')
        if mapped:
            private = executable.parent / 'lib'
            mpv = [Path(path) for path in mapped if Path(path).name.startswith('libmpv.so')]
            if len(mpv) != 1 or not mpv[0].is_relative_to(private):
                raise ValueError(f'GUI did not load exactly the packaged mpv: {mpv}')
            evidence['observed_libraries_sha256'] = {path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
                                                     for path in sorted(mapped)}
        evidence['status'] = 'passed'
    finally:
        if process and process.poll() is None:
            if os.name == 'nt':
                subprocess.run(['taskkill', '/pid', str(process.pid), '/T', '/F'], check=False)
            else:
                os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=10)
        (args.output / 'packaged-gui-check.json').write_text(json.dumps(evidence, indent=2) + '\n')


if __name__ == '__main__':
    main()
