#!/usr/bin/env python3
"""Bridge the Make-selected paths to later release packaging/evidence steps."""
import argparse
import json
import os
from pathlib import Path


def record(env):
    platform = env['VSF_NATIVE_PLATFORM']
    path = Path('target/native') / platform / 'build-paths.json'
    values = {name: str(Path(env[name]).resolve()) for name in
              ('VSF_NATIVE_CACHE', 'CARGO_TARGET_DIR')}
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(values, indent=2) + '\n')
    temporary.replace(path)


def export(path, destination):
    values = json.loads(path.read_text())
    if set(values) != {'VSF_NATIVE_CACHE', 'CARGO_TARGET_DIR'}:
        raise ValueError('Unexpected native build path fields')
    lines = []
    for name, value in values.items():
        if not isinstance(value, str) or '\n' in value or '\r' in value or not Path(value).is_absolute():
            raise ValueError(f'Invalid absolute build path: {name}')
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
