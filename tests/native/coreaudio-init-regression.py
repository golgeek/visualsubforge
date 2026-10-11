#!/usr/bin/env python3
"""Check native CoreAudio initialization cleanup in an explicitly selected libmpv.

macOS only. Builds a standalone probe and test-only DYLD interposer. Never
installs a library, changes an audio device, or modifies application settings.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library-dir', type=Path, required=True)
    parser.add_argument('--include-dir', type=Path, required=True)
    parser.add_argument('--media', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This probe requires macOS CoreAudio and DYLD interposition')
    library = (args.library_dir / 'libmpv.2.dylib').resolve(strict=True)
    media = args.media.resolve(strict=True)
    include = args.include_dir.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = Path(__file__).resolve().parent
    tracer = output / 'interpose.dylib'
    probe = output / 'probe'
    commands = [
        ['clang', '-std=c11', '-Wall', '-Wextra', '-Werror', '-dynamiclib',
         str(source / 'coreaudio-init-interpose.c'), '-framework', 'AudioUnit',
         '-framework', 'CoreAudio', '-o', str(tracer)],
        ['clang', '-std=c11', '-Wall', '-Wextra', '-Werror',
         str(source / 'coreaudio-init-probe.c'), '-I' + str(include), str(library),
         '-o', str(probe)],
    ]
    manifest = {
        'library': str(library), 'library_sha256': digest(library),
        'media': str(media), 'media_sha256': digest(media), 'compile': commands,
        'source_sha256': {name: digest(source / name) for name in (
            'coreaudio-init-probe.c', 'coreaudio-init-interpose.c', Path(__file__).name)},
        'scope': 'Native API resource lifetimes under injected initialization errors; '
                 'not playback continuity, all initialization branches, memory totals or device timing.',
    }
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    for index, command in enumerate(commands):
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        (output / f'compile-{index}.log').write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f'Compile {index} failed: see {output}')
    results = []
    for mode in ('failure', 'format-failure', 'success'):
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(('VSF_', 'DYLD_'))}
        env.update(DYLD_LIBRARY_PATH=str(library.parent),
                   DYLD_INSERT_LIBRARIES=str(tracer), DYLD_PRINT_LIBRARIES='1')
        if mode != 'success':
            env['VSF_AUDIO_FAILURE'] = mode
        command = [str(probe), str(media), mode]
        try:
            result = subprocess.run(command, env=env, capture_output=True,
                                    text=True, timeout=65)
            log = result.stdout + result.stderr
            units = re.findall(r'UNIT_COUNTS created=(\d+) disposed=(\d+) failures=(\d+)', log)
            listeners = re.findall(r'LISTENER_COUNTS added=(\d+) removed=(\d+)', log)
            dispose_errors = re.findall(r'DISPOSE_ERRORS count=(\d+)', log)
            counts = list(map(int, units[-1])) if units else None
            listener_counts = list(map(int, listeners[-1])) if listeners else None
            errors = int(dispose_errors[-1]) if dispose_errors else None
            expected = [2, 2, 0] if mode == 'success' else [10, 10, 10]
            passed = (result.returncode == 0 and counts == expected and errors == 0
                      and listener_counts is not None
                      and listener_counts[0] == listener_counts[1]
                      and str(library) in log)
            row = dict(mode=mode, command=command, exit=result.returncode,
                       units=counts, listeners=listener_counts,
                       dispose_errors=errors, passed=passed)
        except subprocess.TimeoutExpired as error:
            # subprocess.run kills and waits for this owned child on timeout.
            log = (error.stdout or b'').decode(errors='replace') + (error.stderr or b'').decode(errors='replace')
            row = dict(mode=mode, command=command, timeout=True, passed=False)
        (output / f'{mode}.log').write_text(log)
        results.append(row)
        (output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print(json.dumps(row), flush=True)
    return 0 if all(row['passed'] for row in results) else 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(f'CoreAudio initialization regression could not finish: {error}') from error
