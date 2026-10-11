#!/usr/bin/env python3
"""Correlate isolated PipeWire capture cycles and player callback snapshots.

Reports graph delivery and coarse fixture content only. These are neither
hardware presentation timestamps nor a correction for the application's clock.
"""
from array import array
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import re
import sys


def analyze(folder):
    meta = json.loads((folder / 'output.f32.json').read_text())
    if (meta['capture_kind'] != 'pipewire-explicit-stream-graph' or meta['channels'] != 1
            or meta['sample_rate'] != 48000 or meta['failed'] or meta['dropped']):
        raise ValueError('invalid or incomplete capture')
    # Current evidence is ARM64 little-endian on both recording and analysis hosts.
    # Reject another analysis host rather than silently reinterpreting native PCM.
    if sys.byteorder != 'little':
        raise ValueError('this diagnostic currently requires little-endian evidence/host')
    samples = array('f')
    samples.frombytes((folder / 'output.f32').read_bytes())
    if len(samples) != meta['frames'] or not samples or not all(map(math.isfinite, samples)):
        raise ValueError('PCM length/content mismatch')
    blocks = meta['blocks']
    offset = 0
    for block in blocks:
        if (block['offset'] != offset or block['frames'] <= 0 or block['cycle_ns'] <= 0
                or block['time_result'] != 0 or block['rate_num'] != 1 or block['rate_denom'] != 48000):
            raise ValueError('capture block timeline is incomplete')
        offset += block['frames']
    if offset != len(samples):
        raise ValueError('capture blocks do not cover the PCM')
    native = (folder / 'native.log').read_text()
    callbacks = [{k: int(v) for k, v in re.findall(r'(\w+)=(-?\d+)', line)}
                 for line in native.splitlines() if ' VSF_PW local=' in line]
    headers = [{k: int(v) for k, v in re.findall(r'(\w+)=(-?\d+)', line)}
               for line in native.splitlines() if ' VSF_PW_TRACE ' in line]
    if len(headers) != 1 or headers[0]['dropped'] or headers[0]['count'] != len(callbacks):
        raise ValueError('player callback trace is incomplete')
    by_cycle = {}
    for row in callbacks:
        by_cycle.setdefault(row['now'], []).append(row)
    matched = sum(b['cycle_ns'] in by_cycle for b in blocks)
    intervals = []
    discontinuities = []
    unresolved_intervals = []
    for a, b in zip(blocks, blocks[1:]):
        rows_a = by_cycle.get(a['cycle_ns'], [])
        rows_b = by_cycle.get(b['cycle_ns'], [])
        # Pause/restart and missing cycles need a separate report: excluding them
        # from contiguous intervals must not make disrupted delivery look clean.
        if len(rows_a) != 1 or len(rows_b) != 1:
            unresolved_intervals.append({
                'sample_offset': b['offset'], 'cycle_ns': b['cycle_ns'],
                'previous_callback_matches': len(rows_a), 'callback_matches': len(rows_b),
                'graph_interval_ms': (b['cycle_ns'] - a['cycle_ns']) / 1e6,
                'capture_receive_interval_ms': (b['received_ns'] - a['received_ns']) / 1e6,
            })
            continue
        x, y = rows_a[0], rows_b[0]
        if y['ticks'] - x['ticks'] != a['frames']:
            discontinuities.append({
                'sample_offset': b['offset'], 'cycle_ns': b['cycle_ns'],
                'previous_frames': a['frames'], 'tick_delta': y['ticks'] - x['ticks'],
                'graph_interval_ms': (b['cycle_ns'] - a['cycle_ns']) / 1e6,
                'capture_receive_interval_ms': (b['received_ns'] - a['received_ns']) / 1e6,
            })
            continue
        intervals.append({
            'sample_offset': b['offset'], 'cycle_ns': b['cycle_ns'],
            'graph_excess_ms': (b['cycle_ns'] - a['cycle_ns']) / 1e6 - a['frames'] / 48,
            'capture_dispatch_ms': (b['received_ns'] - b['cycle_ns']) / 1e6,
            'player_delay_frames': y['delay'],
        })
    intervals.sort(key=lambda r: r['graph_excess_ms'], reverse=True)
    # Exact zeros/repeated nonzero blocks are observable content, not proof of
    # device silence or a decoder defect. Silence and periodic audio can be valid.
    zero_blocks, repeated_blocks = [], []
    previous = None
    for block in blocks:
        values = samples[block['offset']:block['offset'] + block['frames']]
        entry = {'sample_offset': block['offset'], 'frames': block['frames'],
                 'cycle_ns': block['cycle_ns']}
        if not any(values):
            zero_blocks.append(entry)
        elif previous is not None and values == previous:
            repeated_blocks.append(entry)
        previous = values
    counts, sequence, unknown = Counter(), [], 0
    for start in range(0, len(samples) - 959, 960):
        values = samples[start:start + 960]
        if sum(v * v for v in values) / 960 <= .0001:
            continue
        crossings = [i - a / (b - a) for i, (a, b) in enumerate(zip(values, values[1:])) if a <= 0 < b]
        hz = (len(crossings) - 1) * 48000 / (crossings[-1] - crossings[0]) if len(crossings) > 2 else 0
        tone = min((440, 660, 880), key=lambda t: abs(t - hz))
        if abs(tone - hz) >= 10:
            unknown += 1
            continue
        counts[tone] += 1
        if not sequence or sequence[-1] != tone:
            sequence.append(tone)
    return {'capture_blocks': len(blocks), 'player_callbacks': len(callbacks),
            'matched_graph_cycles': matched, 'metadata_headers': sum(b['header'] for b in blocks),
            'frames': len(samples), 'tone_sequence_hz': sequence,
            'tone_seconds': {str(t): n * .02 for t, n in counts.items()},
            'unclassified_windows': unknown, 'largest_contiguous_graph_intervals': intervals[:3],
            'noncontiguous_graph_steps': len(discontinuities),
            'noncontiguous_graph_step_examples': discontinuities[:5],
            'unresolved_graph_intervals': len(unresolved_intervals),
            'unresolved_graph_interval_examples': unresolved_intervals[:5],
            'all_zero_blocks': len(zero_blocks), 'all_zero_block_examples': zero_blocks[:5],
            'repeated_nonzero_blocks': len(repeated_blocks),
            'repeated_nonzero_block_examples': repeated_blocks[:5],
            'timing_basis': 'graph delivery; not acoustic or device presentation',
            'property_result': json.loads((folder / 'result.json').read_text())}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('folders', type=Path, nargs='+')
    args = parser.parse_args()
    for folder in args.folders:
        print(json.dumps({'folder': str(folder), **analyze(folder)}))
