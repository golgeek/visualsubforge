#!/usr/bin/env python3
"""Collect an explicit set of synthetic fixtures for manual candidate acceptance."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

from native_evidence import digest

ROOT = Path(__file__).resolve().parent.parent
SRT = '7\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>\n\n9\n00:00:04,100 --> 00:00:05,000\nAfter 100 ms\n\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixtures', type=Path, default=ROOT / 'tests/fixtures/generated')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    # Never archive the generated directory wholesale: local experiments can
    # contain user media, old logs or unrelated third-party reference films.
    for relative in ['cfr-offset.mkv', 'vfr-offset.mkv', 'h264-1080p.mp4', 'hevc-4k.mp4',
                     'resync/source.mkv', 'resync/offset.mkv', 'resync/speed-ads.mkv']:
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(args.fixtures / relative, target)
    (output / 'resync/source.srt').write_text(SRT, encoding='utf-8')
    for name, first, second in [
        ('offset', '00:00:06,000 --> 00:00:07,000', '00:00:07,100 --> 00:00:08,000'),
        ('speed-ads', '00:00:01,960 --> 00:00:02,920', '00:00:03,016 --> 00:00:03,880'),
    ]:
        expected = SRT.replace('00:00:03,000 --> 00:00:04,000', first)
        expected = expected.replace('00:00:04,100 --> 00:00:05,000', second)
        (output / f'resync/{name}.expected.srt').write_text(expected, encoding='utf-8')
    (output / 'playback.srt').write_text(
        '1\n00:00:01,000 --> 00:00:03,000\nVSF preview: <i>Café</i>\n\n'
        '2\n00:00:06,000 --> 00:00:08,000\nThe colours invert at six seconds.\n\n', encoding='utf-8')
    shutil.copy2(ROOT / 'docs/preview-acceptance.md', output / 'README.md')
    shutil.copy2(ROOT / 'LICENSE', output / 'LICENSE')
    recipes = output / 'recipes'
    recipes.mkdir()
    for name in ['media-fixtures.py', 'playback-fixtures.py', 'resync-fixtures.py']:
        shutil.copy2(ROOT / 'scripts' / name, recipes / name)
    (output / 'provenance.json').write_text(json.dumps({
        'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'scope': 'Synthetic media from the included VSF generators; no third-party reference films',
        'commands_from_source_checkout': ['python scripts/media-fixtures.py',
                                          'python scripts/playback-fixtures.py',
                                          'python scripts/resync-fixtures.py'],
        'scope_limit': 'Small deterministic controls; not general resync accuracy or codec/device certification',
    }, indent=2) + '\n')
    (output / 'SHA256SUMS').write_text(''.join(f'{digest(path)}  {path.relative_to(output).as_posix()}\n'
        for path in sorted(output.rglob('*')) if path.is_file()))


if __name__ == '__main__':
    main()
