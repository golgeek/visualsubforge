#!/usr/bin/env python3
"""Bridge the Make-selected paths to later release packaging/evidence steps."""
import argparse
import json
import os
import re
from pathlib import Path


LIBRARIES = {'macos': ('ANGLE', 'PLACEBO', 'MPV'),
             'linux': ('FFMPEG', 'DOVI', 'SHADERS', 'PLACEBO', 'MPV'), 'windows': ('FFMPEG', 'MPV', 'MPV_SDK')}
PATH_FIELDS = {'CARGO_TARGET_DIR'} | {f'VSF_{name}_BUILD' for names in LIBRARIES.values() for name in names}
KEY_FIELDS = {f'VSF_{name}_KEY' for names in LIBRARIES.values() for name in names}


def record(env):
    platform = env['VSF_NATIVE_PLATFORM']
    path = Path('target/native') / platform / 'build-paths.json'
    names = LIBRARIES[platform]
    values = {name: str(Path(env[name]).resolve()) for name in
              ('CARGO_TARGET_DIR', *(f'VSF_{lib}_BUILD' for lib in names))}
    values.update({f'VSF_{lib}_KEY': env[f'VSF_{lib}_KEY'] for lib in names})
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(values, indent=2) + '\n')
    temporary.replace(path)


def export(path, destination):
    values = json.loads(path.read_text())
    expected = [{'CARGO_TARGET_DIR'} | {f'VSF_{lib}_{suffix}' for lib in names for suffix in ('BUILD', 'KEY')}
                for names in LIBRARIES.values()]
    if set(values) not in expected:
        raise ValueError('Unexpected native build path fields')
    lines = []
    for name, value in values.items():
        if not isinstance(value, str) or '\n' in value or '\r' in value:
            raise ValueError(f'Invalid build value: {name}')
        if name in PATH_FIELDS and not Path(value).is_absolute():
            raise ValueError(f'Invalid absolute build path: {name}')
        if name in KEY_FIELDS and not re.fullmatch(r'[0-9a-f]{64}', value):
            raise ValueError(f'Invalid cache key: {name}')
        lines.append(f'{name}={value}\n')
    with destination.open('a', encoding='utf-8') as stream:
        stream.writelines(lines)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('record', 'github'))
    parser.add_argument('paths', type=Path, nargs='?')
    args = parser.parse_args()
    if args.action == 'record':
        record(os.environ)
    elif args.paths:
        export(args.paths, Path(os.environ['GITHUB_ENV']))
    else:
        parser.error('github requires the build-paths.json file')
