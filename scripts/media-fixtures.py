#!/usr/bin/env python3
"""Generate synthetic task-003 fixtures and verify their timestamp contracts."""
import argparse
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--ffmpeg', default='ffmpeg')
parser.add_argument('--ffprobe', default='ffprobe')
parser.add_argument('--output', type=Path, default=Path('tests/fixtures/generated'))
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
commands = []


def run(command):
    commands.append(command)
    result = subprocess.run(command, capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f'{command[0]} exited {result.returncode}: {result.stderr}')
    return result.stdout


def probe(path, entries, selector):
    return json.loads(run([args.ffprobe, '-v', 'error', '-select_streams', selector,
                           '-show_entries', entries, '-of', 'json', str(path)]))


versions = {name: run([tool, '-version']).splitlines()[0]
            for name, tool in [('ffmpeg', args.ffmpeg), ('ffprobe', args.ffprobe)]}
base = [args.ffmpeg, '-v', 'error', '-nostdin', '-y']
video = ['-f', 'lavfi', '-i', 'testsrc2=size=160x90:rate=50:duration=4']
audio = ['-itsoffset', '0.25', '-f', 'lavfi', '-i',
         r'aevalsrc=if(between(t\,1\,1.02)\,0.75\,0):s=8000:d=3.75',
         '-f', 'lavfi', '-i',
         r'aevalsrc=if(between(t\,2\,2.02)\,-0.5\,0):s=8000:d=4']
for name, selection in [('cfr-offset.mkv', 'not(mod(n,2))'),
                        ('vfr-offset.mkv', 'if(lt(t,2),not(mod(n,2)),not(mod(n,5)))')]:
    path = args.output / name
    run(base + video + audio + ['-map', '0:v:0', '-map', '1:a:0', '-map', '2:a:0',
        '-vf', f'select={selection.replace(",", chr(92) + ",")}', '-fps_mode', 'vfr',
        '-c:v', 'ffv1', '-c:a', 'pcm_s16le', str(path)])
    pts = [float(frame['best_effort_timestamp_time']) for frame in
           probe(path, 'frame=best_effort_timestamp_time', 'v:0')['frames']]
    expected = ([i / 25 for i in range(100)] if name.startswith('cfr') else
                [i / 25 for i in range(50)] + [2 + i / 10 for i in range(20)])
    if len(pts) != len(expected) or any(abs(a-b) > 0.001 for a, b in zip(pts, expected)):
        raise RuntimeError(f'{name}: unexpected video timestamps: {pts}')
    streams = probe(path, 'stream=start_time', 'a')['streams']
    if [float(s['start_time']) for s in streams] != [0.25, 0.0]:
        raise RuntimeError(f'{name}: unexpected audio offsets: {streams}')

for name, metadata in [('zero-start.wav', []), ('bwf-zero-start.wav',
        ['-write_bext', '1', '-metadata', 'time_reference=480000'])]:
    path = args.output / name
    run(base + ['-f', 'lavfi', '-i',
        r'aevalsrc=if(between(t\,1.25\,1.27)\,0.75\,0):s=8000:d=4',
        '-c:a', 'pcm_s16le', *metadata, str(path)])
    first = json.loads(run([args.ffprobe, '-v', 'error', '-select_streams', 'a:0',
        '-read_intervals', '%+#1', '-show_entries',
        'packet=pts:stream=codec_name:format=format_name,nb_streams:format_tags=time_reference',
        '-of', 'json', str(path)]))
    if (first['format']['format_name'] != 'wav' or
            first['format']['nb_streams'] != 1 or
            first['streams'][0]['codec_name'] != 'pcm_s16le' or
            first['packets'] != [{'pts': 0}]):
        raise RuntimeError(f'{name}: unexpected PCM WAV timestamp contract: {first}')
    if metadata and first['format']['tags'].get('time_reference') != '480000':
        raise RuntimeError(f'{name}: missing nonzero BWF recording metadata')

run(base + ['-f', 'lavfi', '-i',
    r'aevalsrc=if(between(t\,7199\,7199.02)\,0.75\,0):s=8000:d=7200',
    '-c:a', 'flac', str(args.output / 'long.flac')])
long_duration = float(json.loads(run([args.ffprobe, '-v', 'error', '-show_entries',
    'format=duration', '-of', 'json', str(args.output / 'long.flac')]))['format']['duration'])
if abs(long_duration - 7200) > 0.001:
    raise RuntimeError(f'Unexpected long fixture duration: {long_duration}')
(args.output / 'generation.json').write_text(json.dumps({'versions': versions, 'commands': commands}, indent=2) + '\n')
print(json.dumps(versions))
print('Verified CFR/VFR timestamps, two audio offsets, PCM WAV/BWF zero PTS, and 7200-second duration.')
