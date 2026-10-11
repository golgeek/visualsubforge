#!/usr/bin/env python3
"""Compare standalone WASAPI device positions with process-loopback tone markers.

Both timestamp sources use the machine QPC. This is not an acoustic measurement.
Usage: python wasapi-timing-analysis.py CASE_DIRECTORY
"""
from array import array
from bisect import bisect_right
import importlib.util
import json
import math
from pathlib import Path
import statistics
import sys


def analyze(folder):
    report = json.loads((folder / 'report.json').read_text())
    case = json.loads((folder / 'case.json').read_text())
    meta = json.loads((folder / 'output.f32.json').read_text())
    if (not report['ok'] or case.get('player_exit') != 0 or case.get('capture_exit') != 0
            or case.get('forced_cleanup')):
        raise ValueError('native playback/capture did not finish normally')
    spec = importlib.util.spec_from_file_location('tone_content', Path(__file__).with_name('audio-tone-content.py'))
    content = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(content)
    content_result = content.analyze_content(folder / 'output.f32', 'windows-process-loopback')
    pcm = array('f')
    pcm.frombytes((folder / 'output.f32').read_bytes())
    if sys.byteorder != 'little':
        pcm.byteswap()
    if not all(math.isfinite(v) for v in pcm):
        raise ValueError('nonfinite captured sample')
    packets = meta['packets']
    consumed = 0
    for p in packets:
        if p['frame_offset'] != consumed or p['frames'] <= 0 or p['flags'] & 4:
            raise ValueError('invalid capture packet/timestamp')
        consumed += p['frames']
    if consumed != meta['frames']:
        raise ValueError('packet lengths do not cover capture')
    starts = [p['frame_offset'] for p in packets]
    def timestamp(frame):
        p = packets[bisect_right(starts, frame) - 1]
        return p['qpc_100ns'] / 1e7 + (frame - p['frame_offset']) / 48000
    qpc_frequency, clock_frequency = report['qpc_frequency'], report['clock_frequency']
    if qpc_frequency <= 0 or clock_frequency <= 0 or report['sample_rate'] != 48000:
        raise ValueError('invalid render clock configuration')
    epochs = report['epochs']
    expected = 1 if case['mode'] == 'ordinary' else 2
    if len(epochs) != expected or [e['index'] for e in epochs] != list(range(expected)):
        raise ValueError('unexpected stream reset sequence')
    for s in report['samples']:
        if (s['after_qpc'] < s['before_qpc'] or s['status'] not in (0, 1)
                or s['epoch'] not in range(expected)):
            raise ValueError('invalid device-clock sample')
    for epoch in epochs:
        group = [s for s in report['samples'] if s['epoch'] == epoch['index']]
        if len(group) < 2 or any(b['position'] < a['position'] or b['written'] < a['written']
                                 or b['clock_qpc_100ns'] < a['clock_qpc_100ns']
                                 for a, b in zip(group, group[1:])):
            raise ValueError('device clock/submission counters moved backwards within epoch')
    epoch = epochs[-1]
    after = epoch['after_qpc'] / qpc_frequency
    windows = []
    for start in range(0, meta['frames'] - 960 + 1, 960):
        values = pcm[2 * start:2 * (start + 960):2]
        rms = math.sqrt(sum(v * v for v in values) / 960)
        if rms <= .01:
            continue
        crossings = [i - a / (b - a) for i, (a, b) in enumerate(zip(values, values[1:])) if a <= 0 < b]
        if len(crossings) <= 2:
            continue
        hz = (len(crossings) - 1) * 48000 / (crossings[-1] - crossings[0])
        windows.append({'start': timestamp(start), 'end': timestamp(start + 960), 'hz': hz})
    group = [s for s in report['samples'] if s['epoch'] == epoch['index']]
    markers = []
    for frame, old_hz, new_hz in [(72000, 440, 660), (120000, 660, 880)]:
        if epoch['source_start'] >= frame:
            raise ValueError('reset skipped expected source marker')
        target = (frame - epoch['source_start']) * clock_frequency / 48000
        crossing = next(((a, b) for a, b in zip(group, group[1:]) if a['position'] < target <= b['position']), None)
        if crossing is None:
            raise ValueError('device clock did not cross marker')
        a, b = crossing
        if a['status'] or b['status']:
            raise ValueError('device reports imprecise marker-crossing sample')
        # Include query-call bounds as well as the correlated device timestamps.
        predicted = [min(a['clock_qpc_100ns'] / 1e7, a['before_qpc'] / qpc_frequency),
                     max(b['clock_qpc_100ns'] / 1e7, b['after_qpc'] / qpc_frequency)]
        tones = [[w for w in windows if w['start'] > after and abs(w['hz'] - hz) < 10]
                 for hz in (old_hz, new_hz)]
        if not all(tones):
            raise ValueError('captured source marker unavailable')
        old, new = tones[0][-1], tones[1][0]
        if old['end'] > new['start']:
            raise ValueError('captured tone order/timestamps are ambiguous')
        captured = [old['start'], new['end']]
        markers.append({'source_frame': frame, 'stream_frame': frame - epoch['source_start'],
                        'device_crossing_qpc_seconds': predicted, 'capture_marker_qpc_seconds': captured,
                        'capture_minus_device_seconds': [captured[0] - predicted[1], captured[1] - predicted[0]]})
    settled = [s for s in group if after + .5 < s['after_qpc'] / qpc_frequency < after + 2.5]
    if len(settled) < 10:
        raise ValueError('insufficient settled clock observations')
    x = [s['clock_qpc_100ns'] / 1e7 for s in settled]
    y = [s['position'] / clock_frequency for s in settled]
    jumps = [{'qpc_seconds': b['qpc_100ns'] / 1e7,
              'difference_from_pcm_seconds': (b['qpc_100ns'] - a['qpc_100ns']) / 1e7 - a['frames'] / 48000}
             for a, b in zip(packets, packets[1:])
             if abs((b['qpc_100ns'] - a['qpc_100ns']) / 1e7 - a['frames'] / 48000) > .002]
    return {'mode': case['mode'], 'content': content_result, 'markers': markers,
            'device_seconds_per_qpc_second': statistics.linear_regression(x, y).slope,
            'samples': len(report['samples']), 'imprecise_clock_samples': sum(s['status'] == 1 for s in report['samples']),
            'epoch_source_starts': [e['source_start'] for e in epochs],
            'buffer_frames': report['buffer_frames'], 'clock_frequency': clock_frequency,
            'qpc_frequency': qpc_frequency, 'capture_device_positions_all_zero': all(p['device_frame'] == 0 for p in packets),
            'capture_discontinuity_flags': sum(bool(p['flags'] & 1) for p in packets),
            'capture_timestamp_jumps': jumps,
            'median_queue_frames_from_clock': statistics.median(s['written'] - s['position'] * 48000 / clock_frequency for s in settled),
            'median_padding_frames': statistics.median(s['padding'] for s in settled),
            'limits': 'Shared-mode Windows VM process-loopback observation; no mpv, fitted offset, or acoustic presentation assertion.'}


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: wasapi-timing-analysis.py CASE_DIRECTORY')
    print(json.dumps(analyze(Path(sys.argv[1])), indent=2))
