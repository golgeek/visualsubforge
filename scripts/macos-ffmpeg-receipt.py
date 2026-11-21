#!/usr/bin/env python3
"""Record the archive inputs, installed binaries and the static dav1d object targets."""
import argparse
import json
from pathlib import Path
import re
import subprocess

from native_evidence import file_identity


def capture(build):
    sdk = build / 'sdk'
    archive = sdk / 'lib/libdav1d.a'
    commands = subprocess.check_output(['otool', '-l', archive], text=True)
    members = re.split(r'(?m)^.*libdav1d\.a\(([^)]+)\):\s*$', commands)
    targets = []
    for member, details in zip(members[1::2], members[2::2], strict=True):
        target = re.search(r'cmd LC_BUILD_VERSION\s+cmdsize \d+\s+platform (?:1|MACOS|macos)\s+minos ([\d.]+)', details)
        if not target or target[1] not in ('15.0', '15.0.0'):
            raise ValueError(f'Unexpected dav1d deployment target for {member}')
        targets.append({'member': member, 'minimum_macos': target[1]})
    if not targets:
        raise ValueError('No dav1d Mach-O members in static library')
    binaries = [sdk / 'bin/ffmpeg', sdk / 'bin/ffprobe']
    binaries.extend(p for p in sorted((sdk / 'lib').glob('*.dylib')) if not p.is_symlink())
    receipt = {'status': 'passed', 'sources': [file_identity(build / name) for name in ('ffmpeg.tar.xz', 'dav1d.tar.xz')],
               'configure_options': (build / 'configure-options.txt').read_text().splitlines(),
               'dav1d': {**file_identity(archive), 'member_minimum_macos': targets},
               'binaries': [file_identity(p) for p in binaries]}
    (build / 'build-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('build', type=Path)
    capture(parser.parse_args().build.resolve(strict=True))
