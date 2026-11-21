"""Compare the installed Windows media runtime with the original packaged encoders."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from codec_parity_edit import edited_srt


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


def resync_parity(before, after, fixtures, output, encoder):
    # Both installed CLIs must recover the synthetic edit with each video codec.
    for name in ('source', 'offset', 'speed-ads'):
        run([before / 'ffmpeg.exe', '-v', 'error', '-nostdin', '-i', fixtures / f'{name}.mkv',
             '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, output / f'{name}.mkv'])
    results = {}
    for name in ('offset', 'speed-ads'):
        expected = (fixtures / f'{name}.expected.srt').read_text(encoding='utf-8')
        for label, bundle in [('before', before)] + ([('after', after)] if after else []):
            target = output / f'{name}-{label}.srt'
            result = run([bundle / 'vsf.exe', 'resync', '--source-video', output / 'source.mkv',
                          '--source-subtitles', fixtures / 'source.srt', '--target-video', output / f'{name}.mkv',
                          '--output', target, '--report', target.with_suffix('.json'), '--json'])
            if json.loads(result.stdout)['status'] != 'complete' or target.read_text(encoding='utf-8') != expected:
                raise ValueError(f'{name} resync differs from the expected subtitle text or timings ({label})')
        results[name] = hashlib.sha256(expected.encode()).hexdigest()
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--after', type=Path, help='Installed candidate to compare')
    mode.add_argument('--recreate-pr39', type=Path, help='Downloaded original Windows parity evidence')
    for name in ['before', 'diagnostics', 'fixtures', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(exist_ok=False)
    if args.recreate_pr39:
        context = json.loads((args.recreate_pr39 / 'context.json').read_text())
        if context['fixture_run'] != '43650131492' or context['package_run'] != '44834011693':
            raise ValueError('Expected the original PR 39 evidence')
        for name, checksum in context['downloads']['fixtures'].items():
            fixture_input = (args.fixtures / name).resolve(strict=True)
            if not fixture_input.is_relative_to(args.fixtures.resolve()) or hashlib.sha256(fixture_input.read_bytes()).hexdigest() != checksum:
                raise ValueError(f'PR 39 fixture input differs: {name}')
    for codec, encoder in [('vp8', 'libvpx'), ('vp9', 'libvpx-vp9')]:
        if args.recreate_pr39:
            record = json.loads((args.recreate_pr39 / 'codec-parity' / f'{codec}.json').read_text())
            if hashlib.sha256((args.before / 'ffmpeg.exe').read_bytes()).hexdigest() != record['before_ffmpeg_sha256']:
                raise ValueError('PR 39 reference encoder differs')
        fixture = args.output / (codec + '.mkv')
        # The old package supplies the independent encoders for this comparison.
        run([args.before / 'ffmpeg.exe', '-nostdin', '-i', args.fixtures / 'vfr-offset.mkv',
             '-map', '0:v', '-map', '0:a', '-vf', "drawbox=color=white:t=fill:enable='gte(t,2)'",
             '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, '-c:a', 'libopus', fixture])
        baseline, _ = observe(args.before, fixture, codec, args.diagnostics.resolve())
        if args.recreate_pr39:
            output = args.output / codec
            output.mkdir()
            baseline['resync_srt_sha256'] = resync_parity(args.before, None, args.fixtures / 'resync', output, encoder)
            observations = {'pr39': record['identical'], 'replay': baseline,
                            'pr39_fixture_sha256': record['fixture_sha256'],
                            'replay_fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest()}
            (output / 'observations.json').write_text(json.dumps(observations, indent=2) + '\n', encoding='utf-8')
            if baseline != record['identical']:
                raise ValueError(f'Recreated {codec} observations differ from recorded PR 39 results')
            # Matroska assigns a fresh segment UID on encode. Retain the actual
            # bytes consumed by Windows and pass those same files to Linux.
            (output / 'windows.json').write_text(json.dumps({
                'recorded': record, 'edited_srt_sha256': edited_srt(args.before / 'vsf.exe', args.fixtures, output),
                'files': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in [fixture, *sorted(output.glob('*.mkv'))]}}, indent=2) + '\n')
            continue
        candidate, logs = observe(args.after, fixture, codec, args.diagnostics.resolve())
        if baseline != candidate:
            raise ValueError(f'Decoded frame/audio hashes, waveform or cuts differ for {codec}')
        output = args.output / codec
        subprocess.run([sys.executable, Path(__file__).with_name('packaged-gui.py'), '--executable',
                        args.after / 'visualsubforge.exe', '--fixture', fixture, '--output', output,
                        '--environment', 'Hosted Windows, software D3D11, virtual WASAPI'], check=True)
        candidate['resync_srt_sha256'] = resync_parity(args.before, args.after, args.fixtures / 'resync', output, encoder)
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
