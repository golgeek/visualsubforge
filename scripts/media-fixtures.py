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
# Original embedded-text corpus: shared by CLI, Rust and native review.
subtitles = args.output / 'subtitle-extraction'
subtitles.mkdir(exist_ok=True)
(subtitles / 'text.srt').write_text('1\n00:00:01,000 --> 00:00:02,000\nFirst\n\n2\n00:00:03,000 --> 00:00:04,000\nSecond\n')
(subtitles / 'text.vtt').write_text('WEBVTT\n\n00:00:01.000 --> 00:00:02.000 line:10%\n<v Test>First\n')
(subtitles / 'text.ass').write_text("""[Script Info]
; Original extraction fixture header
ScriptType: v4.00+
PlayResX: 160
PlayResY: 90
[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding
Style: Default,VSF Fixture Font,24,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,1,0,2,10,10,10,1
Style: Signs,VSF Fixture Font,18,&H0000FFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,1,0,8,10,10,10,1
[Events]
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Comment: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,Header comment
Dialogue: 1,0:00:03.00,0:00:04.00,Signs,,0,0,0,,BA
Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,AB
""")
run(base + ['-i', 'tests/fixtures/subtitles/styles.ssa', '-c:s', 'copy', str(subtitles / 'ssa.mkv')])
font = Path('tests/fixtures/subtitles/fixture-font.ttf')
inputs = sum((['-i', str(subtitles / ('text.' + ext))] for ext in ['srt', 'ass', 'vtt']), [])
run(base + ['-f', 'lavfi', '-i', 'testsrc2=size=160x90:rate=25:duration=5',
    '-f', 'lavfi', '-i', 'sine=frequency=440:duration=5'] + inputs +
    ['-map', '0:v', '-map', '1:a', '-map', '2:s', '-map', '3:s', '-map', '4:s',
     '-c:v', 'ffv1', '-c:a', 'pcm_s16le', '-c:s', 'copy',
     '-metadata:s:s:0', 'language=fra', '-metadata:s:s:0', 'title=French SRT',
     '-metadata:s:s:1', 'language=eng', '-disposition:s:0', 'default',
     '-attach', str(font), '-metadata:s:t:0', 'mimetype=application/x-truetype-font',
     '-output_ts_offset', '1.5', str(subtitles / 'offset.mkv')])
run(base + ['-f', 'lavfi', '-i', 'testsrc2=size=160x90:rate=25:duration=5',
    '-i', str(subtitles / 'text.srt'), '-c:v', 'mpeg4', '-c:s', 'mov_text', str(subtitles / 'text.mp4')])
(subtitles / 'ten-thousand.srt').write_text(''.join(
    f'{i+1}\n00:{i//600:02}:{(i//10)%60:02},{(i%10)*100:03} --> 00:{(i+1)//600:02}:{((i+1)//10)%60:02},{((i+1)%10)*100:03}\nCue {i+1}\n\n'
    for i in range(10000)))
run(base + ['-i', str(subtitles / 'ten-thousand.srt'), '-c:s', 'copy', str(subtitles / 'ten-thousand.mkv')])

run(['python3', 'scripts/subtitle-edge-fixtures.py', str(subtitles)])

(args.output / 'generation.json').write_text(json.dumps({'versions': versions, 'commands': commands}, indent=2) + '\n')
print(json.dumps(versions))
print('Verified CFR/VFR timestamps, two audio offsets, PCM WAV/BWF zero PTS, and 7200-second duration.')
