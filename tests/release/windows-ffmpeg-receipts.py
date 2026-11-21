"""Compare packaged FFmpeg receipts bytewise, substituting only each real SDK prefix."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import re
import sys


def sha(data):
    return hashlib.sha256(data).hexdigest()


def receipt(build, package):
    inventory = (package / 'native-inventory.json').read_bytes()
    runtime = json.loads((package / 'native-runtime.json').read_bytes())
    if sha(inventory) != runtime['native_inventory_sha256']:
        raise ValueError(f'Wrong source inventory for {package}')
    checksums = json.loads(inventory)['custom_files']
    options, = build.rglob('configure-options.txt')
    files = {name: options.with_name(name).read_bytes() for name in ('configure-options.txt', 'config.h')}
    for name, data in files.items():
        if sha(data) != checksums[name]:
            raise ValueError(f'{name} differs from the packaged receipt for {runtime["commit"]}')
    prefixes = [line.removeprefix(b'--prefix=') for line in files['configure-options.txt'].splitlines()
                if line.startswith(b'--prefix=')]
    if len(prefixes) != 1 or not re.fullmatch(rb'[A-Za-z]:/[^\r\n\x00]+/target/native/windows/ffmpeg/[a-f0-9]{64}/sdk', prefixes[0]):
        raise ValueError('Expected one absolute Windows FFmpeg cache prefix')
    prefix = prefixes[0]
    if b"--prefix='" + prefix + b"'" not in files['config.h']:
        raise ValueError('config.h does not identify the configured prefix')
    return runtime['commit'], prefix, files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('main-build', 'main-package', 'candidate-build', 'candidate-package', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    main_commit, main_prefix, reference = receipt(args.main_build, args.main_package)
    candidate_commit, candidate_prefix, candidate = receipt(args.candidate_build, args.candidate_package)
    report = {'main_commit': main_commit, 'candidate_commit': candidate_commit,
              'main_prefix': main_prefix.decode(), 'candidate_prefix': candidate_prefix.decode(), 'files': {}}
    args.output.mkdir(parents=True, exist_ok=True)
    for name, original in reference.items():
        current = candidate[name]
        before = original.replace(main_prefix, b'<SDK_PREFIX>')
        after = current.replace(candidate_prefix, b'<SDK_PREFIX>')
        report['files'][name] = {
            'raw': {'main': sha(original), 'candidate': sha(current), 'equal': original == current},
            'normalized': {'main': sha(before), 'candidate': sha(after), 'equal': before == after},
        }
        delta = difflib.diff_bytes(difflib.unified_diff, original.splitlines(True), current.splitlines(True),
                                   f'main/{name}'.encode(), f'candidate/{name}'.encode(), n=0)
        (args.output / (name + '.diff')).write_bytes(b''.join(delta))
    (args.output / 'hashes.json').write_text(json.dumps(report, indent=2) + '\n')
    for name, hashes in report['files'].items():
        if not hashes['normalized']['equal']:
            sys.exit(f'{name} differs outside the exact SDK prefix; see {args.output}')


if __name__ == '__main__':
    main()
