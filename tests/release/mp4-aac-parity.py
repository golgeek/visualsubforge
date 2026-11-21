"""Check retained release MP4 duration, decoded frames and native-rate AAC padding."""
import argparse
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import subprocess


def capture(bin_dir, fixture):
    def run(name, *args):
        command = bin_dir / name
        if not command.exists():
            command = command.with_suffix('.exe')
        return subprocess.check_output([command, '-v', 'error', *args])
    info = json.loads(run('ffprobe', '-show_entries',
                          'format=duration:stream=codec_type,codec_name,channels,sample_rate,duration,start_time',
                          '-of', 'json', fixture))
    audio, = (stream for stream in info['streams'] if stream['codec_type'] == 'audio')
    if (audio['codec_name'] != 'aac' or audio['channels'] != 1 or audio['sample_rate'] != '48000'
            or any(Decimal(x['duration']) != 12 for x in [info['format'], *info['streams']])):
        raise ValueError(f'Unexpected AAC format or duration: {info}')
    pcm = run('ffmpeg', '-nostdin', '-threads', '1', '-i', fixture, '-map', '0:a:0',
              '-threads', '1', '-c:a', 'pcm_s16le', '-f', 's16le', '-')
    framehash = run('ffmpeg', '-nostdin', '-copyts', '-threads', '1', '-i', fixture,
                    '-map', '0:v:0', '-threads', '1', '-c:v', 'rawvideo', '-fps_mode', 'passthrough',
                    '-enc_time_base', '1:1000000', '-f', 'framehash', '-hash', 'sha256', '-').decode()
    if '#tb 0: 1/1000000' not in framehash:
        raise ValueError('Unexpected frame time base')
    frames = [line.strip() for line in framehash.splitlines() if line and not line.startswith('#')]
    if not frames or len(pcm) % 2:
        raise ValueError('Empty video or incomplete PCM')
    return {'probe': info, 'frames': frames, 'samples': len(pcm) // 2,
            'pcm_sha256': hashlib.sha256(pcm).hexdigest()}, pcm


def verify(bin_dir, fixtures, output, before):
    report = {}
    failures = []
    for name in ('h264-1080p.mp4', 'hevc-4k.mp4'):
        fixture = fixtures / name
        new, pcm = capture(bin_dir, fixture)
        with fixture.open('rb') as stream:
            source_hash = hashlib.file_digest(stream, 'sha256').hexdigest()
        row = report[name] = {'file_sha256': source_hash, 'candidate': new}
        (output.parent / (name + '.s16le')).write_bytes(pcm)
        if new['samples'] != 576512:
            failures.append(f'{name}: 8.1.2 returned {new["samples"]} samples')
        if before:
            old, reference = capture(before, fixture)
            row['before'] = old
            row['prefix_identical'] = reference == pcm[:len(reference)]
            if (not row['prefix_identical'] or old['samples'] != 576000 or old['frames'] != new['frames']
                    or old['probe'] != new['probe']):
                failures.append(f'{name}: baseline differs outside the final 512 samples')
    output.write_text(json.dumps({'files': report, 'failures': failures}, indent=2) + '\n')
    if failures:
        raise ValueError('; '.join(failures))


def compare_packages(directory):
    runners = ['macos-15', 'macos-15-intel', 'windows-2022', 'ubuntu-24.04']
    reports = []
    for runner in runners:
        path = directory / ('decoder-parity-' + runner) / 'mp4-aac.json'
        report = json.loads(path.read_bytes())
        if report['failures']:
            raise ValueError(f'{runner}: {report["failures"]}')
        reports.append(report['files'])
    for name in ('h264-1080p.mp4', 'hevc-4k.mp4'):
        reference = reports[0][name]
        for runner, report in zip(runners, reports, strict=True):
            candidate = report[name]['candidate']
            if (report[name]['file_sha256'] != reference['file_sha256'] or candidate['samples'] != 576512
                    or any(candidate[k] != reference['candidate'][k] for k in ('probe', 'frames'))):
                raise ValueError(f'{runner}: retained MP4 bytes, decoded frame/timing or sample count differs: {name}')
        print(f'{name}: four packages, 576512 samples, {len(reference["candidate"]["frames"])} identical frames and timestamps')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest='mode', required=True)
    check = modes.add_parser('decode')
    for name in ('bundle', 'fixtures', 'output'):
        check.add_argument('--' + name, type=Path, required=True)
    check.add_argument('--before', type=Path, help='Previous packaged Homebrew FFmpeg')
    combine = modes.add_parser('compare')
    combine.add_argument('directory', type=Path)
    args = parser.parse_args()
    if args.mode == 'compare':
        compare_packages(args.directory)
    else:
        verify(args.bundle, args.fixtures, args.output, args.before)
