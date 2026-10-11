#!/usr/bin/env python3
"""Check synthetic frame markers against source PTS and probe seek requests.

For scripts/seek-frame-fixture.py only. The expected image covers the requested
time in a half-open [PTS, next PTS) interval. This checks engine snapshots, not
display presentation or audio alignment. Missing/ambiguous images fail.
"""
import argparse
from bisect import bisect_right
from fractions import Fraction
import json
from pathlib import Path
import re
import subprocess


def decode_markers(ffmpeg, path):
    command = [ffmpeg, '-v', 'error', '-i', str(path), '-an', '-vf',
               'crop=192:32:0:0', '-pix_fmt', 'gray', '-fps_mode', 'passthrough',
               '-f', 'rawvideo', '-']
    data = subprocess.run(command, check=True, capture_output=True).stdout
    if not data or len(data) % (192 * 32):
        raise ValueError('Missing or incomplete marker image')
    markers = []
    for offset in range(0, len(data), 192 * 32):
        means = []
        for cell in range(12):
            values = [data[offset + y * 192 + cell * 16 + x]
                      for y in range(8, 24) for x in range(4, 12)]
            if max(values) - min(values) > 16:
                raise ValueError('Nonuniform marker cell')
            means.append(sum(values) / len(values))
        if any(64 <= value <= 192 for value in means):
            raise ValueError('Ambiguous marker cell')
        bits = [int(value > 192) for value in means]
        if bits[:2] != [1, 0]:
            raise ValueError('Invalid white/black marker anchors')
        markers.append(sum(bit << i for i, bit in enumerate(bits[2:])))
    return markers


def check_sequence(args, pts):
    """Check completed steps, restoration and gross resume jumps."""
    log = (args.case / 'stdout.log').read_text()
    observations = re.findall(
        r'^RESUME_OBSERVATION cycle=(\d) before=(\d+) after=(\d+) '
        r'elapsed_us=(\d+) paused=(\d) passed=(\d)$', log, re.MULTILINE)
    cycle = args.sequence != 'plain'
    names = ['initial', 'next', 'previous', 'resumed'] if cycle else ['initial', 'resumed']
    if args.sequence == 'restore':
        names.append('restored')
    images, errors = {}, []
    for name in names:
        try:
            ids = decode_markers(args.ffmpeg, args.case / f'{name}.png')
            if len(ids) != 1 or not 0 <= ids[0] < len(pts):
                raise ValueError('Expected one valid fixture frame')
            images[name] = ids[0]
        except (ValueError, subprocess.CalledProcessError) as error:
            errors.append(f'{name}: {error}')
    if len(observations) != 1:
        errors.append('Expected one completed resume observation')
    else:
        mode, before, after, elapsed, paused, passed = map(int, observations[0])
        if mode != int(cycle) or paused != 1 or passed != 1 or not 0 < after - before <= elapsed + 250000:
            errors.append('Reported resume progression failed')
        if all(name in images for name in names):
            if cycle and (images['next'] != images['initial'] + 1 or
                          images['previous'] != images['initial']):
                errors.append('Next/previous images do not form a one-frame cycle')
            origin = images['previous'] if cycle else images['initial']
            advance = pts[images['resumed']] - pts[origin]
            if not 0 < advance <= Fraction(elapsed + 250000, 1000000):
                errors.append('Actual resumed image advances beyond the elapsed-time bound or does not advance')
    if cycle:
        steps = re.findall(r'^STEP forward=(\d) status=(-?\d+) point=(-?\d+) delivered=(\d) passed=(\d)$',
                           log, re.MULTILINE)
        if len(steps) != 2 or any(int(row[0]) != direction or row[1] != '0' or row[3:] != ('1', '1')
                                 for row, direction in zip(steps, [1, 0])):
            errors.append('Missing successful completion/paused-point observations for both steps')
    if args.sequence == 'restore':
        restored = re.findall(r'^RESTORE_RESULT before=(\d+) after=(\d+) completed=(\d) passed=(\d)$',
                              log, re.MULTILINE)
        if len(restored) != 1:
            errors.append('Expected one completed restoration observation')
        else:
            before, after, completed, passed = map(int, restored[0])
            if completed != 1 or passed != 1 or abs(after - before) > 2:
                errors.append('Restored clock differs from the previous-frame timestamp')
            if 'restored' in images and abs(Fraction(after, 1000000) - pts[images['restored']]) > Fraction(2, 1000000):
                errors.append('Restored clock does not identify the captured source frame')
        if 'restored' not in images or images['restored'] != images.get('previous'):
            errors.append('Restored image differs from the previous frame')
    if args.sequence == 'queued':
        requests = re.findall(r'^QUEUED_RESTORE target=(\d+) result=(-?\d+)$', log, re.MULTILINE)
        completions = re.findall(r'^QUEUED_RESTORE_DONE count=(\d+)$', log, re.MULTILINE)
        if (len(requests) != 1 or requests[0][1] != '0' or len(observations) != 1 or
                requests[0][0] != observations[0][1]):
            errors.append('Missing accepted queued restore to the resume origin')
        if len(completions) != 1 or int(completions[0]) < 1:
            errors.append('Missing queued seek completion')
    return dict(sequence=args.sequence, images=images, errors=errors, passed=not errors)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--case', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--ffmpeg', default='ffmpeg')
    parser.add_argument('--ffprobe', default='ffprobe')
    parser.add_argument('--sequence', choices=['plain', 'cycle', 'restore', 'queued'],
                        help='Check step/resume snapshots instead of three seeks')
    args = parser.parse_args()
    info = json.loads(subprocess.run([
        args.ffprobe, '-v', 'error', '-select_streams', 'v:0', '-show_frames',
        '-show_streams', '-show_entries', 'stream=time_base:frame=best_effort_timestamp',
        '-of', 'json', str(args.fixture)], check=True, capture_output=True).stdout)
    time_base = Fraction(info['streams'][0]['time_base'])
    pts = [int(frame['best_effort_timestamp']) * time_base for frame in info['frames']]
    source_ids = decode_markers(args.ffmpeg, args.fixture)
    if source_ids != list(range(360)) or len(pts) != 360:
        raise ValueError('Source markers/frame count do not match the fixture contract')
    if any(value != Fraction(i, 30) for i, value in enumerate(pts)):
        raise ValueError('Source PTS do not match the 30 fps fixture contract')
    if args.sequence:
        result = check_sequence(args, pts)
        report = dict(source_frames_verified=len(source_ids),
                      contract='One-frame cycle; resume advance bounded by elapsed time plus 250 ms',
                      results=[result])
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report))
        return 0 if result['passed'] else 1
    targets = [int(value) for value in re.findall(
        r'^SEEK_RESULT (?:mode=\S+ )?target=(\d+)',
        (args.case / 'stdout.log').read_text(), re.MULTILINE)]
    if len(targets) != 3:
        raise ValueError('Expected three completed seek results')
    results = []
    for index, target in enumerate(targets, 1):
        expected = bisect_right(pts, Fraction(target, 1000000)) - 1
        if not 0 <= target < 12000000:
            raise ValueError('Target outside the fixture')
        image = args.case / f'frame-{index}.png'
        try:
            ids = decode_markers(args.ffmpeg, image)
            if len(ids) != 1:
                raise ValueError('Expected one snapshot frame')
            row = dict(target_us=target, expected_frame=expected,
                       actual_frame=ids[0], passed=ids[0] == expected)
        except (ValueError, subprocess.CalledProcessError) as error:
            row = dict(target_us=target, expected_frame=expected,
                       passed=False, error=str(error))
        results.append(row)
    report = dict(source_frames_verified=len(source_ids),
                  contract='Frame covering target in [PTS, next PTS)', results=results)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))
    return 0 if all(row['passed'] for row in results) else 1


if __name__ == '__main__':
    raise SystemExit(main())
