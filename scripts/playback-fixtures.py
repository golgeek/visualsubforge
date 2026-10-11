#!/usr/bin/env python3
"""Generate SDR compressed playback fixtures, with a known visual cut at 6 s."""
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
    result = subprocess.run(command, capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout

versions = {tool: run([value, '-version']).splitlines()[0]
            for tool, value in [('ffmpeg', args.ffmpeg), ('ffprobe', args.ffprobe)]}
for name, size, encoder, extra in [
    ('h264-1080p.mp4', '1920x1080', 'libx264', ['-threads', '4']),
    ('hevc-4k.mp4', '3840x2160', 'libx265', ['-x265-params', 'pools=4:frame-threads=2', '-tag:v', 'hvc1']),
]:
    path = args.output / name
    run([args.ffmpeg, '-v', 'error', '-nostdin', '-y', '-f', 'lavfi', '-i',
         f'testsrc2=size={size}:rate=30:duration=12', '-f', 'lavfi', '-i',
         'sine=frequency=440:sample_rate=48000:duration=12', '-vf', "negate=enable='gte(t,6)'",
         '-c:v', encoder, '-preset', 'ultrafast', '-crf', '26', '-pix_fmt', 'yuv420p',
         '-g', '60', *extra, '-c:a', 'aac', '-b:a', '96k', '-movflags', '+faststart', str(path)])
    streams = json.loads(run([args.ffprobe, '-v', 'error', '-select_streams', 'v:0',
        '-show_entries', 'stream=codec_name,width,height,nb_frames,duration', '-of', 'json', str(path)]))['streams']
    stream = streams[0]
    if stream['nb_frames'] != '360' or abs(float(stream['duration']) - 12) > .001:
        raise RuntimeError(f'Unexpected fixture contract: {streams}')
    print(name, stream, flush=True)
(args.output / 'playback-generation.json').write_text(json.dumps(
    {'versions': versions, 'commands': commands, 'cut_seconds': 6, 'color': 'SDR 8-bit synthetic'}, indent=2) + '\n')
