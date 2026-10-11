#!/usr/bin/env python3
"""Generate longer, independently encoded codec/container/color test signals."""
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent.parent
out = root / 'tests/fixtures/generated'
out.mkdir(parents=True, exist_ok=True)
ffmpeg = '/opt/homebrew/bin/ffmpeg'
ffprobe = '/opt/homebrew/bin/ffprobe'
commands = []
def run(cmd):
    commands.append(cmd)
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    if p.returncode:
        raise RuntimeError(p.stderr)
    return p.stdout

cases = [
    ('long-gop-h264.mkv', 180, '1280x720', 25,
     ['-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '28', '-threads', '4',
      '-g', '250', '-keyint_min', '250', '-sc_threshold', '0', '-pix_fmt', 'yuv420p']),
    ('hevc-main10-pq.mkv', 90, '1920x1080', 30,
     ['-c:v', 'libx265', '-preset', 'ultrafast', '-crf', '28', '-pix_fmt', 'yuv420p10le',
      '-x265-params', 'pools=4:frame-threads=2:keyint=150:min-keyint=150:scenecut=0',
      '-color_primaries', 'bt2020', '-color_trc', 'smpte2084', '-colorspace', 'bt2020nc']),
    ('fallback-ffv1.mkv', 30, '640x360', 24,
     ['-c:v', 'ffv1', '-threads', '4', '-pix_fmt', 'yuv420p']),
]
records = []
for name, seconds, size, fps, args in cases:
    path = out / name
    run([ffmpeg, '-v', 'error', '-nostdin', '-y', '-f', 'lavfi', '-i',
         f'testsrc2=size={size}:rate={fps}:duration={seconds}', '-f', 'lavfi', '-i',
         f'sine=frequency=880:sample_rate=48000:duration={seconds}', *args,
         '-c:a', 'flac', str(path)])
    probe = json.loads(run([ffprobe, '-v', 'error', '-show_streams', '-show_format', '-of', 'json', str(path)]))
    if abs(float(probe['format']['duration']) - seconds) > .1:
        raise RuntimeError(f'Duration mismatch: {name}')
    records.append({'name': name, 'probe': probe})
    print(name, flush=True)
(out/'extended-generation.json').write_text(json.dumps({'commands':commands, 'fixtures':records,
    'versions':{t:run([t,'-version']).splitlines()[0] for t in [ffmpeg,ffprobe]},
    'note':'PQ-tagged synthetic signal tests decoding/tone-mapping path, not mastered HDR visual accuracy.'},indent=2)+'\n')
