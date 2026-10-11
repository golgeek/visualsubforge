#!/usr/bin/env python3
"""Cross-check experiment packet types/PTS against FFmpeg's actual decoder."""
import argparse
import csv
import io
import json
from pathlib import Path
import subprocess
from study import compare

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('extractor', type=Path)
parser.add_argument('fixtures', type=Path)
args = parser.parse_args()
assert compare([1, 1.0001], [1])['matched'] == 1
assert compare([.96, 1.04], [1])['matched'] == 0
for filename in ('scene-fixed-gop.mkv', 'scene-motion-portrait.mkv'):
    path = args.fixtures / filename
    result = subprocess.run([str(args.extractor.resolve()), str(path)], capture_output=True,
                            text=True, check=True, timeout=30)
    packets = list(csv.DictReader(io.StringIO(result.stdout)))
    decoded = json.loads(subprocess.check_output([
        'ffprobe', '-v', 'error', '-select_streams', 'V:0', '-show_frames',
        '-show_entries', 'frame=pts_time,pict_type,key_frame', '-of', 'json', str(path)], timeout=30))['frames']
    actual = sorted((round(float(p['pts']), 6), p['type'], int(p['key'])) for p in packets)
    expected = sorted((round(float(p['pts_time']), 6), p['pict_type'], p['key_frame']) for p in decoded)
    assert actual == expected, filename
    assert not any(p['type'] == 'I' and p['key'] == '0' for p in packets), filename
    print(f'{filename}: {len(packets)} PTS/type/key triples match decoded frames; no non-key I candidates')
for filename in ('scene-cfr.mkv', 'scene-audio.wav', 'missing-input.mkv'):
    result = subprocess.run([str(args.extractor.resolve()), str(args.fixtures / filename)],
                            capture_output=True, text=True, timeout=30)
    assert result.returncode != 0 and result.stderr.strip(), filename
    print(f'{filename}: explicit rejection')
print('One-to-one matching checks passed')
