"""Compare two installed media runtimes on identical synthetic packets and SRT inputs."""
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import struct
import subprocess
import sys

from codec_parity_audio import compare_platform_audio
from codec_parity_edit import edited_srt


def run(command, env=None):
    result = subprocess.run(command, env=env, capture_output=True, timeout=240)
    if result.returncode:
        raise ValueError(f'{command}: exit {result.returncode}: {result.stderr.decode(errors="replace")[-3000:]}')
    return result.stdout


def tool(directory, name):
    return directory / (name + '.exe' if os.name == 'nt' else name)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def release_media(bundle, fixtures):
    # Hash in FFmpeg so a complete decoded 4K stream never resides in Python.
    expected = {
        'h264-1080p.mp4': ['h264', 'aac'], 'hevc-4k.mp4': ['hevc', 'aac'],
        'cfr-offset.mkv': ['ffv1', 'pcm_s16le', 'pcm_s16le'],
        'vfr-offset.mkv': ['ffv1', 'pcm_s16le', 'pcm_s16le'],
    }
    results = {}
    for name, codecs in expected.items():
        fixture = fixtures / name
        streams = json.loads(run([tool(bundle, 'ffprobe'), '-v', 'error', '-show_entries',
                                  'stream=index,codec_name,codec_type', '-of', 'json', fixture]))['streams']
        if [s['codec_name'] for s in streams] != codecs:
            raise ValueError(f'{name}: expected {codecs}, found {streams}')
        hashes = []
        for stream in streams:
            options = (['-c:v', 'rawvideo', '-fps_mode', 'passthrough'] if stream['codec_type'] == 'video'
                       else ['-c:a', 'pcm_s16le'])
            value = run([tool(bundle, 'ffmpeg'), '-v', 'error', '-nostdin', '-threads', '1', '-i', fixture,
                         '-map', f"0:{stream['index']}", *options, '-threads', '1',
                         '-f', 'hash', '-hash', 'sha256', '-']).decode().strip()
            if not value.startswith('SHA256=') or value[7:] == digest(b''):
                raise ValueError(f'{name}: empty or missing decoded hash: {value}')
            hashes.append({'codec': stream['codec_name'], 'sha256': value[7:]})
        results[name] = hashes
    return results


def observe(bundle, fixture, diagnostics, pcm_output=None):
    env = {k: v for k, v in os.environ.items() if not k.startswith(('LD_', 'DYLD_', 'VSF_'))}
    system_dirs = [str(Path(os.environ['SystemRoot']) / 'System32')] if os.name == 'nt' else ['/usr/bin', '/bin']
    env.update(PATH=os.pathsep.join([str(bundle), *system_dirs]), VSF_FFMPEG=str(tool(bundle, 'ffmpeg')),
               VSF_FFPROBE=str(tool(bundle, 'ffprobe')))
    observed = {}
    for stream, fmt in [('v:0', 'rawvideo'), ('a:0', 's16le'), ('a:1', 's16le')]:
        timing = ['-fps_mode', 'passthrough'] if stream == 'v:0' else []
        data = run([tool(bundle, 'ffmpeg'), '-nostdin', '-i', fixture, '-map', '0:' + stream,
                    '-threads', '1', *timing, '-f', fmt, '-'], env)
        if not data:
            raise ValueError(f'No decoded data for {stream}')
        observed[stream] = digest(data)
        if pcm_output and stream.startswith('a:'):
            track = stream.split(':')[1]
            (pcm_output / f'track-{track}-native.s16le').write_bytes(data)
            native = run([tool(bundle, 'ffmpeg'), '-nostdin', '-i', fixture, '-map', '0:' + stream,
                          '-threads', '1', '-c:a', 'pcm_f32le', '-f', 'f32le', '-'], env)
            (pcm_output / f'track-{track}-native.f32le').write_bytes(native)
    framehash = run([tool(bundle, 'ffmpeg'), '-v', 'error', '-nostdin', '-copyts', '-i', fixture,
                     '-map', '0:v:0', '-threads', '1', '-c:v', 'rawvideo', '-fps_mode', 'passthrough',
                     '-enc_time_base', '1:1000000', '-f', 'framehash', '-hash', 'sha256', '-'], env).decode()
    if '#tb 0: 1/1000000' not in framehash:
        raise ValueError('Missing decoded frame timestamp scale')
    frames = []
    for line in framehash.splitlines():
        if line and not line.startswith('#'):
            _, _, pts, _, size, checksum = [part.strip() for part in line.split(',')]
            frames.append({'pts_us': int(pts), 'bytes': int(size), 'sha256': checksum})
    if not frames:
        raise ValueError('Missing decoded video frames')
    observed['frames'] = frames
    observed['waveform'] = [json.loads(run([tool(diagnostics, 'media-analysis'), fixture, track], env))
                            for track in ('0', '1')]
    if pcm_output:
        for track, wave in enumerate(observed['waveform']):
            (pcm_output / f'track-{track}-8k.f32le').write_bytes(waveform_samples(bundle, fixture, wave))
    observed['cuts'] = json.loads(run([tool(diagnostics / 'examples', 'scene-scan'), fixture], env))['cuts']
    return observed


def waveform_samples(bundle, fixture, waveform):
    # Reproduce the actual production bins before comparing the PCM on a
    # common clock. Keep any difference other than an initial silence visible.
    origin = waveform['container_origin_seconds']
    pcm = run([tool(bundle, 'ffmpeg'), '-v', 'error', '-nostdin', '-threads', '2', '-copyts', '-i', fixture,
               '-map', f"0:{waveform['audio_stream_index']}", '-vn', '-sn', '-dn', '-ac', '1',
               '-filter_threads', '1', '-af', f'asetpts=PTS-({origin})/TB,aresample=8000:async=1:first_pts=0',
               '-ar', '8000', '-threads', '1', '-f', 'f32le', 'pipe:1'])
    samples = [v for v, in struct.iter_unpack('<f', pcm)]
    size = waveform['samples_per_peak']
    peaks = [(min(samples[i:i+size]), max(samples[i:i+size])) for i in range(0, len(samples), size)]
    reported = [(p['min'], p['max']) for p in waveform['peaks']]
    if len(samples) != waveform['sample_count'] or len(peaks) != len(reported) or any(
            a != struct.unpack('<ff', struct.pack('<ff', *b)) for a, b in zip(peaks, reported)):
        raise ValueError('Decoded PCM does not reproduce production waveform bins')
    return pcm


def waveform_alignment(before, after, fixture, results):
    evidence = []
    for left, right in zip(results['before']['waveform'], results['after']['waveform'], strict=True):
        first = waveform_samples(before, fixture, left)
        second = waveform_samples(after, fixture, right)
        delta = (right['container_origin_seconds'] - left['container_origin_seconds']) * 8000
        shift = round(delta)
        if abs(delta - shift) > 0.000001:
            raise ValueError(f'Nonintegral sample offset: {delta}')
        early, late = (first, second) if shift >= 0 else (second, first)
        leading = early[:abs(shift)*4]
        evidence.append({'before_sha256': digest(first), 'after_sha256': digest(second),
                         'offset_samples': shift, 'first_samples_silent': not any(leading),
                         'aligned_samples_identical': early[len(leading):] == late})
    return evidence


def subtitles(bundle, fixtures, media, output):
    results = {}
    cli = tool(bundle, 'vsf')
    results['edited'] = edited_srt(cli, fixtures, output)
    for name in ('offset', 'speed-ads'):
        target = output / f'{name}.srt'
        report = json.loads(run([cli, 'resync', '--source-video', media / 'source.mkv',
            '--source-subtitles', fixtures / 'resync/source.srt', '--target-video', media / f'{name}.mkv',
            '--output', target, '--report', target.with_suffix('.json'), '--json']))
        if report['status'] != 'complete' or target.read_text(encoding='utf-8') != (fixtures / f'resync/{name}.expected.srt').read_text(encoding='utf-8'):
            raise ValueError(f'Resynchronized text or timings differ for {name}')
        results[name] = digest(target.read_bytes())
    return results


def ubuntu_timing_matches(report):
    before, after = (report['observations'][key] for key in ('before', 'after'))
    old, new = report['ffmpeg_versions']
    # This exception applies only to the measured 7 ms Matroska Opus pre-skip.
    # Frame hashes and absolute frame PTS are part of the strict comparison.
    if not (old.startswith('ffmpeg version 6.1.1') and new.startswith('ffmpeg version 8.1.2')
            and before['cuts'] == [2.007] and after['cuts'] == [2.0]):
        return False
    if {k: v for k, v in before.items() if k not in ('cuts', 'waveform')} != {
            k: v for k, v in after.items() if k not in ('cuts', 'waveform')}:
        return False
    for left, right, samples in zip(before['waveform'], after['waveform'], report['waveform_alignment'], strict=True):
        if not (left['container_origin_seconds'] == -0.007 and right['container_origin_seconds'] == 0
                and left['sample_count'] == right['sample_count'] + 56 and samples['offset_samples'] == 56
                and samples['first_samples_silent'] and samples['aligned_samples_identical']
                and all(left[k] == right[k] for k in ('samples_per_peak', 'sample_rate', 'audio_stream_index'))):
            return False
    return True


def encode(ffmpeg, fixtures, output):
    output.mkdir(parents=True, exist_ok=False)
    for codec, encoder in [('vp8', 'libvpx'), ('vp9', 'libvpx-vp9')]:
        work = output / codec
        work.mkdir()
        fixture = work / 'cut.mkv'
        run([ffmpeg, '-nostdin', '-i', fixtures / 'vfr-offset.mkv',
             '-map', '0:v', '-map', '0:a', '-vf', "drawbox=color=white:t=fill:enable='gte(t,2)'",
             '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, '-c:a', 'libopus', fixture])
        for name in ('source', 'offset', 'speed-ads'):
            run([ffmpeg, '-nostdin', '-i', fixtures / f'resync/{name}.mkv',
                 '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, work / f'{name}.mkv'])
    for name in ('plugins', 'subtitles'):
        shutil.copytree(fixtures / name, output / 'inputs' / name)
    resync = output / 'inputs/resync'
    resync.mkdir()
    for name in ('source.srt', 'offset.expected.srt', 'speed-ads.expected.srt'):
        shutil.copy2(fixtures / 'resync' / name, resync / name)
    (output / 'SHA256SUMS').write_text(''.join(
        f'{digest(p.read_bytes())}  {p.relative_to(output).as_posix()}\n'
        for p in sorted(output.rglob('*')) if p.is_file()), encoding='utf-8')


def verify_packets(packets):
    manifest = packets / 'SHA256SUMS'
    expected = {p.relative_to(packets).as_posix() for p in packets.rglob('*') if p.is_file() and p != manifest}
    seen = set()
    for line in manifest.read_text(encoding='utf-8').splitlines():
        sha, name = line.split('  ', 1)
        if name not in expected or name in seen or digest((packets / name).read_bytes()) != sha:
            raise ValueError(f'Invalid shared fixture checksum: {name}')
        seen.add(name)
    if seen != expected:
        raise ValueError('Incomplete shared fixture manifest')


def timestamps(bundle, fixture):
    return json.loads(run([tool(bundle, 'ffprobe'), '-v', 'error', '-show_entries',
        'format=start_time,duration:stream=index,codec_name,start_time,time_base', '-of', 'json', fixture]))


def package_observation(bundle, packets, codec, diagnostics, output, *, save_pcm=False):
    fixture = packets / codec / 'cut.mkv'
    output.mkdir(parents=True)
    observed = observe(bundle, fixture, diagnostics, output if save_pcm else None)
    observed['subtitles'] = subtitles(bundle, packets / 'inputs', fixture.parent, output)
    observed['timestamps'] = timestamps(bundle, fixture)
    return observed


def compare(args):
    verify_packets(args.packets)
    args.output.mkdir(parents=True, exist_ok=False)
    version = run([tool(args.bundle, 'ffmpeg'), '-version']).decode().splitlines()[0]
    if os.name == 'nt':
        # This build embeds the enclosing checkout's Git revision in its
        # banner. The published inventory binds its binaries and source.
        inventory_file = args.bundle / 'native-inventory.json'
        receipt = json.loads((args.bundle / 'native-runtime.json').read_bytes())
        if digest(inventory_file.read_bytes()) != receipt['native_inventory_sha256']:
            raise ValueError('Windows source inventory differs from the package receipt')
        inventory = json.loads(inventory_file.read_bytes())
        if inventory['custom_files']['ffmpeg-8.1.2.tar.xz'] != '464beb5e7bf0c311e68b45ae2f04e9cc2af88851abb4082231742a74d97b524c':
            raise ValueError('Windows FFmpeg source differs from the pinned 8.1.2 archive')
        files = {entry['path']: entry['sha256'] for entry in inventory['files']}
        for name in ('ffmpeg.exe', 'ffprobe.exe'):
            if files[name] != digest((args.bundle / name).read_bytes()):
                raise ValueError(f'Windows package inventory mismatch: {name}')
    elif not version.startswith('ffmpeg version 8.1.2 '):
        raise ValueError(f'Unexpected packaged FFmpeg: {version}')
    results = {'manifest_sha256': digest((args.packets / 'SHA256SUMS').read_bytes()), 'codecs': {}}
    differences = []
    baseline = args.before or args.macos_before
    if baseline:
        media = {label: release_media(bundle, args.fixtures) for label, bundle in
                 [('before', baseline), ('after', args.bundle)]}
        (args.output / 'release-media.json').write_text(json.dumps(media, indent=2) + '\n')
        for name, streams in media['before'].items():
            for old_stream, new_stream in zip(streams, media['after'][name], strict=True):
                # The separate MP4 gate checks the complete prefix, tail,
                # sample counts, frame hashes and PTS on the same files.
                if args.macos_before and old_stream['codec'] == new_stream['codec'] == 'aac':
                    continue
                if old_stream != new_stream:
                    differences.append(f'{name} {old_stream["codec"]} decoded stream')
        if args.macos_before:
            run([sys.executable, Path(__file__).with_name('mp4-aac-parity.py'), 'decode',
                 '--bundle', args.bundle, '--before', baseline, '--fixtures', args.fixtures,
                 '--output', args.output / 'mp4-aac.json'])
    for codec in ('vp8', 'vp9'):
        candidate = package_observation(args.bundle, args.packets, codec, args.diagnostics, args.output / codec / 'after',
                                        save_pcm=args.save_pcm or bool(args.windows) or os.name == 'nt')
        results['codecs'][codec] = candidate
        if candidate['cuts'] != [2.0] or candidate['timestamps']['format']['start_time'] != '0.000000':
            differences.append(f'{codec} candidate cut or container origin')
        if args.macos_before:
            old = package_observation(args.macos_before, args.packets, codec, args.diagnostics,
                                      args.output / codec / 'before', save_pcm=True)
            (args.output / f'{codec}-mac-before.json').write_text(json.dumps(old, indent=2) + '\n')
            for field in old:
                if old[field] != candidate[field]:
                    differences.append(f'{codec} macOS reference {field}')
        if args.before:
            fixture = args.packets / codec / 'cut.mkv'
            old = package_observation(args.before, args.packets, codec, args.diagnostics, args.output / codec / 'before')
            # The historical gate records probe metadata separately. Its 7 ms
            # exception never applies to the Windows/Linux comparison below.
            previous = {k: v for k, v in old.items() if k != 'timestamps'}
            current = {k: v for k, v in candidate.items() if k != 'timestamps'}
            pair = {'before': previous, 'after': current}
            report = {'fixture_sha256': digest(fixture.read_bytes()), 'observations': pair,
                      'timestamps': {'before': old['timestamps'], 'after': candidate['timestamps']},
                      'ffmpeg_versions': [run([tool(args.before, 'ffmpeg'), '-version']).decode().splitlines()[0], version],
                      'waveform_alignment': waveform_alignment(args.before, args.bundle, fixture, pair)}
            (args.output / f'{codec}.json').write_text(json.dumps(report, indent=2) + '\n')
            if previous != current and not ubuntu_timing_matches(report):
                differences.append(f'{codec} Ubuntu reference')
    (args.output / 'observations.json').write_text(json.dumps(results, indent=2) + '\n', encoding='utf-8')
    if args.windows:
        audio = compare_platform_audio(args.windows, args.output / 'observations.json')
        (args.output / 'cross-platform-audio.json').write_text(json.dumps(audio, indent=2) + '\n')
        differences.extend(audio['failures'])
    if differences:
        raise ValueError('Parity differences: ' + ', '.join(differences))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest='mode', required=True)
    generate = modes.add_parser('encode')
    for name in ('ffmpeg', 'fixtures', 'output'):
        generate.add_argument('--' + name, type=Path, required=True)
    observe_parser = modes.add_parser('observe')
    for name in ('bundle', 'packets', 'diagnostics', 'output'):
        observe_parser.add_argument('--' + name, type=lambda value: Path(value).resolve(), required=True)
    references = observe_parser.add_mutually_exclusive_group()
    references.add_argument('--macos-before', type=Path, help='Former macOS package, with AAC end-padding check')
    references.add_argument('--before', type=Path, help='Former Ubuntu package')
    observe_parser.add_argument('--fixtures', type=Path, help='Original unencoded release pack for the Ubuntu comparison')
    observe_parser.add_argument('--save-pcm', action='store_true')
    observe_parser.add_argument('--windows', '--reference-observations', type=Path, help='Exact Windows package observations')
    args = parser.parse_args()
    if args.mode == 'encode':
        encode(args.ffmpeg, args.fixtures, args.output)
    else:
        if bool(args.before or args.macos_before) != bool(args.fixtures):
            parser.error('a baseline package and --fixtures must be used together')
        compare(args)


if __name__ == '__main__':
    main()
