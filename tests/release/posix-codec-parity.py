"""Compare two installed media runtimes on identical synthetic packets and SRT inputs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


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
        data = run([bundle / 'ffmpeg', '-nostdin', '-i', fixture, '-map', '0:' + stream,
                    '-threads', '1', '-f', fmt, '-'], env)
        if not data:
            raise ValueError(f'No decoded data for {stream}')
        observed[stream] = digest(data)
    observed['waveform'] = [json.loads(run([diagnostics / 'media-analysis', fixture, track], env))
                            for track in ('0', '1')]
    observed['cuts'] = json.loads(run([diagnostics / 'examples/scene-scan', fixture], env))['cuts']
    return observed


def subtitles(bundle, fixtures, media, output):
    results = {}
    cli = bundle / 'vsf'
    options = ['--subtitles', fixtures / 'subtitles/fansub.srt',
               '--package', fixtures / 'plugins/trim-trailing/manifest.json', '--json']
    review = json.loads(run([cli, 'plugin', 'preview', *options]))
    target = output / 'edited.srt'
    run([cli, 'plugin', 'apply', *options, '--accept', review['reviewId'], '--output', target])
    if target.read_bytes() == (fixtures / 'subtitles/fansub.srt').read_bytes():
        raise ValueError('The edit fixture did not exercise a subtitle change')
    results['edited'] = digest(target.read_bytes())
    for name in ('offset', 'speed-ads'):
        target = output / f'{name}.srt'
        report = json.loads(run([cli, 'resync', '--source-video', media / 'source.mkv',
            '--source-subtitles', fixtures / 'resync/source.srt', '--target-video', media / f'{name}.mkv',
            '--output', target, '--report', target.with_suffix('.json'), '--json']))
        if report['status'] != 'complete' or target.read_text() != (fixtures / f'resync/{name}.expected.srt').read_text():
            raise ValueError(f'Resynchronized text or timings differ for {name}')
        results[name] = digest(target.read_bytes())
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('before', 'after', 'diagnostics', 'fixtures', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
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
        (args.output / f'{codec}.json').write_text(json.dumps(report, indent=2) + '\n')
        for label, result in results.items():
            if result['cuts'] != [2.0]:
                differences.append(f"{codec} {label} cut {result['cuts']} (expected [2.0])")
        if results['before'] != results['after']:
            differences.append(f'{codec} decoded bytes, analysis or SRTs')
    if differences:
        raise ValueError('Parity differences: ' + ', '.join(differences))


if __name__ == '__main__':
    main()
