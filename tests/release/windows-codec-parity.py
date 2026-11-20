"""Compare the installed Windows media runtime with the original packaged encoders."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def run(args, env=None):
    result = subprocess.run(args, env=env, capture_output=True, timeout=180)
    if result.returncode:
        raise ValueError(f'{args[0]} failed: {result.stderr.decode(errors="replace")[-4000:]}')
    return result


def observe(bundle, fixture, codec, diagnostics):
    ffmpeg = bundle / 'ffmpeg.exe'
    # scene-scan selects tools via PATH. Restrict it to this package and the OS.
    env = dict(os.environ, PATH=os.pathsep.join([str(bundle), str(Path(os.environ['SYSTEMROOT']) / 'System32')]),
               VSF_FFMPEG=str(ffmpeg), VSF_FFPROBE=str(bundle / 'ffprobe.exe'))
    observations = {}
    logs = []
    for stream, output, expected in [('v:0', 'rawvideo', codec), ('a:0', 's16le', 'opus'), ('a:1', 's16le', 'opus')]:
        result = run([ffmpeg, '-nostdin', '-i', fixture, '-map', '0:' + stream, '-threads', '1',
                      '-f', output, '-'], env)
        log = result.stderr.decode(errors='replace')
        logs.append(log)
        if not result.stdout or not re.search(r'\(' + expected + r' \(native\) ->', log):
            raise ValueError(f'Expected FFmpeg native {expected} decoder for {stream}: {log}')
        observations[stream] = hashlib.sha256(result.stdout).hexdigest()
    observations['waveform'] = [json.loads(run([diagnostics / 'media-analysis.exe', fixture, track], env).stdout)
                                for track in ('0', '1')]
    observations['cuts'] = json.loads(run([diagnostics / 'examples/scene-scan.exe', fixture], env).stdout)['cuts']
    if not observations['cuts']:
        raise ValueError('Scene fixture produced no cuts')
    return observations, '\n'.join(logs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['before', 'after', 'diagnostics', 'fixtures', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    for codec, encoder in [('vp8', 'libvpx'), ('vp9', 'libvpx-vp9')]:
        fixture = args.output / (codec + '.mkv')
        # The old package supplies the independent encoders for this comparison.
        run([args.before / 'ffmpeg.exe', '-nostdin', '-i', args.fixtures / 'vfr-offset.mkv',
             '-map', '0:v', '-map', '0:a', '-vf', "drawbox=color=white:t=fill:enable='gte(t,2)'",
             '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, '-c:a', 'libopus', fixture])
        baseline, _ = observe(args.before, fixture, codec, args.diagnostics.resolve())
        candidate, logs = observe(args.after, fixture, codec, args.diagnostics.resolve())
        if baseline != candidate:
            raise ValueError(f'Decoded frame/audio hashes, waveform or cuts differ for {codec}')
        output = args.output / codec
        subprocess.run([sys.executable, Path(__file__).with_name('packaged-gui.py'), '--executable',
                        args.after / 'visualsubforge.exe', '--fixture', fixture, '--output', output,
                        '--environment', 'Hosted Windows, software D3D11, virtual WASAPI'], check=True)
        (output / 'ffmpeg.log').write_text(logs)
        mpv = (output / 'stderr.log').read_text(errors='replace')
        for module, name in [('vd', codec), ('ad', 'opus')]:
            if set(re.findall(r'VSF mpv \[' + module + r'\] Opening decoder (\S+)', mpv)) != {name}:
                raise ValueError(f'mpv did not use the expected {name} decoder')
        (args.output / f'{codec}.json').write_text(json.dumps({
            'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(),
            'before_ffmpeg_sha256': hashlib.sha256((args.before / 'ffmpeg.exe').read_bytes()).hexdigest(),
            'after_ffmpeg_sha256': hashlib.sha256((args.after / 'ffmpeg.exe').read_bytes()).hexdigest(),
            'identical': candidate}, indent=2) + '\n')


if __name__ == '__main__':
    main()
