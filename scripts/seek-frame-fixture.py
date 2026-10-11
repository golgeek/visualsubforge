#!/usr/bin/env python3
"""Generate a 12-second 30 fps H.264/AAC clip with font-independent frame IDs.

The top 32 rows contain twelve 16-pixel cells: white/black anchors followed
by the zero-based frame ordinal as ten little-endian binary cells. Read cell
centres to avoid chroma/resizing boundaries. This is synthetic test material.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--ffmpeg', default='ffmpeg')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    path = args.output / 'frame-id.mp4'
    width, height, fps, count = 640, 360, 30, 360
    command = [args.ffmpeg, '-v', 'error', '-nostdin', '-f', 'rawvideo',
               '-pixel_format', 'rgb24', '-video_size', f'{width}x{height}',
               '-framerate', str(fps), '-i', 'pipe:0', '-f', 'lavfi', '-i',
               'sine=frequency=440:sample_rate=48000:duration=12',
               '-c:v', 'libx264', '-preset', 'ultrafast', '-crf', '18',
               '-pix_fmt', 'yuv420p', '-threads', '4', '-g', '60',
               '-c:a', 'aac', '-b:a', '96k', '-movflags', '+faststart', str(path)]
    with (args.output / 'ffmpeg.log').open('wb') as errors:
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stderr=errors)
        try:
            for number in range(count):
                row = bytearray(bytes((32, 64, 96)) * width)
                x = number * 7 % (width - 16)
                row[x * 3:(x + 16) * 3] = bytes((32, 220, 96)) * 16
                frame = row * height
                marker = bytearray()
                for bit in [1, 0] + [(number >> i) & 1 for i in range(10)]:
                    marker.extend(bytes((235 if bit else 16,)) * (16 * 3))
                for y in range(32):
                    frame[y * width * 3:y * width * 3 + len(marker)] = marker
                process.stdin.write(frame)
            process.stdin.close()
            result = process.wait(timeout=60)
        except BaseException:
            process.kill()
            process.wait()
            raise
    if result:
        raise RuntimeError(f'ffmpeg exited {result}; see {args.output / "ffmpeg.log"}')
    version = subprocess.run([args.ffmpeg, '-version'], check=True,
                             capture_output=True, text=True).stdout.splitlines()[0]
    metadata = dict(command=command, ffmpeg=version, width=width, height=height,
                    fps=fps, frames=count, marker='white, black, 10 LSB-first bits',
                    sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                    generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    (args.output / 'generation.json').write_text(json.dumps(metadata, indent=2) + '\n')
    print(path)


if __name__ == '__main__':
    main()
