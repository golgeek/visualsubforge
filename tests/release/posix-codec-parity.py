"""Compare two installed media runtimes on identical synthetic packets and SRT inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess

from codec_parity_edit import edited_srt


def run(command, env=None):
    result = subprocess.run(command, env=env, capture_output=True, timeout=240)
    if result.returncode:
        raise ValueError(f'{command}: exit {result.returncode}: {result.stderr.decode(errors="replace")[-3000:]}')
    return result.stdout


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
        streams = json.loads(run([bundle / 'ffprobe', '-v', 'error', '-show_entries',
                                  'stream=index,codec_name,codec_type', '-of', 'json', fixture]))['streams']
        if [s['codec_name'] for s in streams] != codecs:
            raise ValueError(f'{name}: expected {codecs}, found {streams}')
        hashes = []
        for stream in streams:
            options = (['-c:v', 'rawvideo', '-fps_mode', 'passthrough'] if stream['codec_type'] == 'video'
                       else ['-c:a', 'pcm_s16le'])
            value = run([bundle / 'ffmpeg', '-v', 'error', '-nostdin', '-threads', '1', '-i', fixture,
                         '-map', f"0:{stream['index']}", *options, '-threads', '1',
                         '-f', 'hash', '-hash', 'sha256', '-']).decode().strip()
            if not value.startswith('SHA256=') or value[7:] == digest(b''):
                raise ValueError(f'{name}: empty or missing decoded hash: {value}')
            hashes.append({'codec': stream['codec_name'], 'sha256': value[7:]})
        results[name] = hashes
    return results


def observe(bundle, fixture, diagnostics):
    env = {k: v for k, v in os.environ.items() if not k.startswith(('LD_', 'DYLD_', 'VSF_'))}
    env.update(PATH=f'{bundle}:/usr/bin:/bin', VSF_FFMPEG=str(bundle / 'ffmpeg'),
               VSF_FFPROBE=str(bundle / 'ffprobe'))
    observed = {}
    for stream, fmt in [('v:0', 'rawvideo'), ('a:0', 's16le'), ('a:1', 's16le')]:
        timing = ['-fps_mode', 'passthrough'] if stream == 'v:0' else []
        data = run([bundle / 'ffmpeg', '-nostdin', '-i', fixture, '-map', '0:' + stream,
                    '-threads', '1', *timing, '-f', fmt, '-'], env)
        if not data:
            raise ValueError(f'No decoded data for {stream}')
        observed[stream] = digest(data)
    framehash = run([bundle / 'ffmpeg', '-v', 'error', '-nostdin', '-copyts', '-i', fixture,
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
    observed['waveform'] = [json.loads(run([diagnostics / 'media-analysis', fixture, track], env))
                            for track in ('0', '1')]
    observed['cuts'] = json.loads(run([diagnostics / 'examples/scene-scan', fixture], env))['cuts']
    return observed


def waveform_samples(bundle, fixture, waveform):
    # Reproduce the actual production bins before comparing the PCM on a
    # common clock. Keep any difference other than an initial silence visible.
    origin = waveform['container_origin_seconds']
    pcm = run([bundle / 'ffmpeg', '-v', 'error', '-nostdin', '-threads', '2', '-copyts', '-i', fixture,
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
    cli = bundle / 'vsf'
    results['edited'] = edited_srt(cli, fixtures, output)
    for name in ('offset', 'speed-ads'):
        target = output / f'{name}.srt'
        report = json.loads(run([cli, 'resync', '--source-video', media / 'source.mkv',
            '--source-subtitles', fixtures / 'resync/source.srt', '--target-video', media / f'{name}.mkv',
            '--output', target, '--report', target.with_suffix('.json'), '--json']))
        if report['status'] != 'complete' or target.read_text() != (fixtures / f'resync/{name}.expected.srt').read_text():
            raise ValueError(f'Resynchronized text or timings differ for {name}')
        results[name] = digest(target.read_bytes())
    return results


def compare_pr39(bundle, diagnostics, reference, output):
    for codec in ('vp8', 'vp9'):
        work = reference / codec
        evidence = json.loads((work / 'windows.json').read_text())
        for name, checksum in evidence['files'].items():
            path = (reference if name == f'{codec}.mkv' else work) / name
            if digest(path.read_bytes()) != checksum:
                raise ValueError(f'PR 39 packet checksum differs: {name}')
        fixture = reference / f'{codec}.mkv'
        expected = evidence['recorded']['identical']
        actual = observe(bundle, fixture, diagnostics)
        # PR 39 recorded the default rawvideo output cadence. The regular
        # Linux parity check also compares each individual presentation frame.
        legacy_video = run([bundle / 'ffmpeg', '-nostdin', '-i', fixture, '-map', '0:v:0',
                            '-threads', '1', '-f', 'rawvideo', '-'])
        out = output / f'windows-{codec}'
        out.mkdir()
        srts = subtitles(bundle, reference / 'inputs', work, out)
        historical = {'v:0': digest(legacy_video), 'a:0': actual['a:0'], 'a:1': actual['a:1'],
                      'waveform': actual['waveform'], 'cuts': actual['cuts'],
                      'resync_srt_sha256': {k: srts[k] for k in ('offset', 'speed-ads')}}
        (out / 'observations.json').write_text(json.dumps({'windows': evidence, 'linux': historical,
            'linux_edited_srt_sha256': srts['edited'], 'decoded_frames': actual['frames']}, indent=2) + '\n')
        if historical != expected or srts['edited'] != evidence['edited_srt_sha256']:
            raise ValueError(f'Linux results differ from the Windows PR 39 reference for {codec}')


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('before', 'after', 'diagnostics', 'fixtures', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--pr39', type=Path, help='Verified Windows reference packets for the Ubuntu timing comparison')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    media = {label: release_media(bundle.resolve(), args.fixtures) for label, bundle in
             [('before', args.before), ('after', args.after)]}
    (args.output / 'release-media.json').write_text(json.dumps(media, indent=2) + '\n')
    differences = []
    if media['before'] != media['after']:
        differences.append('release media streams')
    for codec, encoder in [('vp8', 'libvpx'), ('vp9', 'libvpx-vp9')]:
        work = args.output / codec
        work.mkdir()
        fixture = work / 'cut.mkv'
        run([args.before / 'ffmpeg', '-nostdin', '-i', args.fixtures / 'vfr-offset.mkv',
             '-map', '0:v', '-map', '0:a', '-vf', "drawbox=color=white:t=fill:enable='gte(t,2)'",
             '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, '-c:a', 'libopus', fixture])
        for name in ('source', 'offset', 'speed-ads'):
            run([args.before / 'ffmpeg', '-nostdin', '-i', args.fixtures / f'resync/{name}.mkv',
                 '-fps_mode', 'passthrough', '-threads', '1', '-c:v', encoder, work / f'{name}.mkv'])
        results = {}
        for label, bundle in [('before', args.before.resolve()), ('after', args.after.resolve())]:
            out = work / label
            out.mkdir()
            observed = observe(bundle, fixture, args.diagnostics.resolve())
            observed['subtitles'] = subtitles(bundle, args.fixtures, work, out)
            results[label] = observed
        timestamps = {label: json.loads(run([bundle / 'ffprobe', '-v', 'error', '-show_entries',
                      'format=start_time,duration:stream=index,codec_name,start_time,time_base',
                      '-of', 'json', fixture])) for label, bundle in [('before', args.before), ('after', args.after)]}
        report = {'fixture_sha256': digest(fixture.read_bytes()), 'observations': results, 'timestamps': timestamps,
                  'ffmpeg_versions': [run([folder / 'ffmpeg', '-version']).decode().splitlines()[0]
                                      for folder in (args.before, args.after)]}
        report['waveform_alignment'] = waveform_alignment(args.before, args.after, fixture, results)
        (args.output / f'{codec}.json').write_text(json.dumps(report, indent=2) + '\n')
        if args.pr39 and ubuntu_timing_matches(report):
            continue
        for label, result in results.items():
            if result['cuts'] != [2.0]:
                differences.append(f"{codec} {label} cut {result['cuts']} (expected [2.0])")
        if results['before'] != results['after']:
            differences.append(f'{codec} decoded bytes, analysis or SRTs')
    if args.pr39:
        compare_pr39(args.after.resolve(), args.diagnostics.resolve(), args.pr39, args.output)
    if differences:
        raise ValueError('Parity differences: ' + ', '.join(differences))


if __name__ == '__main__':
    main()
