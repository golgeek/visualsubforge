#!/usr/bin/env python3
"""Check the real MSYS2 signature verifier before spending time compiling media."""
import argparse
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts'))
from msys_sources import archive_version, fields, signed_download, verify_signature


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--msys', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    # Brotli is a small installed dependency of the selected media libraries.
    name = 'mingw-w64-clang-x86_64-brotli'
    records = [fields(path.read_text(encoding='utf-8'), database=True)
               for path in (args.msys / 'var/lib/pacman/local').glob(name + '-*/desc')]
    records = [record for record in records if record.get('name') == [name]]
    if len(records) != 1:
        raise ValueError('Expected one installed Brotli package for signature control')
    record = records[0]
    filename = f'{name}-{archive_version(record["version"][0])}-{record["arch"][0]}.pkg.tar.zst'
    archive = signed_download('https://repo.msys2.org/mingw/clang64/' + filename,
                              args.output, args.msys)
    changed = args.output / 'changed-package.pkg.tar.zst'
    changed.write_bytes(archive.read_bytes() + b'changed content\n')
    try:
        verify_signature(changed, Path(str(archive) + '.sig'), args.msys)
    except ValueError:
        print('Signed package accepted; changed package rejected.')
    else:
        raise ValueError('Signature verifier accepted changed package bytes')


if __name__ == '__main__':
    main()
