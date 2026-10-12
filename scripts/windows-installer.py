#!/usr/bin/env python3
"""Build, attach or verify the pinned Windows installer inputs."""
import argparse
import os
from pathlib import Path

from windows_installer import attach, build, verify

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('operation', choices=['build', 'attach', 'verify'])
parser.add_argument('--build', type=Path, required=True)
parser.add_argument('--package', type=Path)
parser.add_argument('--installers', type=Path)
parser.add_argument('--nsis-cache', type=Path)
args = parser.parse_args()
root = args.build.resolve()
if args.operation == 'build':
    build(root)
elif args.operation == 'attach':
    if args.package is None:
        parser.error('--package is required for attach')
    attach(root, args.package.resolve(strict=True))
else:
    if args.installers is None:
        parser.error('--installers is required for verify')
    cache = args.nsis_cache or Path(os.environ['LOCALAPPDATA']) / 'tauri/NSIS'
    verify(root, args.installers.resolve(strict=True), cache)
