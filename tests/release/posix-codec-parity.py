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
    if observed['cuts'] != [2.0]:
        raise ValueError(f"Expected a single 2.000-second cut: {observed['cuts']}")
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
        report = {'fixture_sha256': digest(fixture.read_bytes()), 'observations': results,
                  'ffmpeg_versions': [run([folder / 'ffmpeg', '-version']).decode().splitlines()[0]
                                      for folder in (args.before, args.after)]}
        (args.output / f'{codec}.json').write_text(json.dumps(report, indent=2) + '\n')
        if results['before'] != results['after']:
            raise ValueError(f'Decoded bytes, analysis or SRTs differ for {codec}')


if __name__ == '__main__':
    main()
