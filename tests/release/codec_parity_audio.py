"""Compare the two packaged Opus decodes on a shared sample clock."""
import hashlib
import json
import math
import struct


def sample_differences(windows, linux, scale=1):
    first = None
    largest = 0
    different = 0
    for i, (w, l) in enumerate(zip(windows, linux)):
        if not (math.isfinite(w) and math.isfinite(l)):
            raise ValueError(f'Nonfinite sample at {i}')
        if w != l:
            if first is None:
                first = {'index': i, 'windows': w, 'linux': l}
            different += 1
            largest = max(largest, abs(w - l) * scale)
    return {'windows_count': len(windows), 'linux_count': len(linux), 'first': first,
            'different_in_common_prefix': different, 'max_absolute_difference': largest}


def without(fields, *names):
    return {k: v for k, v in fields.items() if k not in names}


def read_pcm(path, fmt, expected_hash=None):
    data = path.read_bytes()
    if not data or (expected_hash and hashlib.sha256(data).hexdigest() != expected_hash):
        raise ValueError(f'Empty or inconsistent PCM evidence: {path}')
    return [v for v, in struct.iter_unpack(fmt, data)]


def compare_platform_audio(windows_file, linux_file):
    windows, linux = (json.loads(p.read_bytes()) for p in (windows_file, linux_file))
    failures = []
    tracks = {}
    if (without(windows, 'codecs') != without(linux, 'codecs') or
            set(windows['codecs']) != {'vp8', 'vp9'} or set(linux['codecs']) != {'vp8', 'vp9'}):
        failures.append('fixture manifest or codec list')
    for codec in ('vp8', 'vp9'):
        left, right = (obs['codecs'][codec] for obs in (windows, linux))
        if without(left, 'a:0', 'a:1', 'waveform') != without(right, 'a:0', 'a:1', 'waveform'):
            failures.append(f'{codec} exact video, timing or SRT fields')
        if len(left['waveform']) != 2 or len(right['waveform']) != 2:
            failures.append(f'{codec} waveform track count')
        for track, (w, l) in enumerate(zip(left['waveform'], right['waveform'])):
            key = f'{codec}/a:{track}'
            report = tracks[key] = {}
            if without(w, 'peaks') != without(l, 'peaks'):
                failures.append(f'{key} waveform metadata or sample count')
            for name, fmt, scale in [('native.s16le', '<h', 1), ('native.f32le', '<f', 32768),
                                     ('8k.f32le', '<f', 32768)]:
                streams = []
                for path, obs in [(windows_file, left), (linux_file, right)]:
                    pcm_path = path.parent / codec / 'after' / f'track-{track}-{name}'
                    expected = obs[f'a:{track}'] if name == 'native.s16le' else None
                    streams.append(read_pcm(pcm_path, fmt, expected))
                stats = report[name] = sample_differences(*streams, scale)
                if len(streams[0]) != len(streams[1]) or stats['max_absolute_difference'] > 2:
                    failures.append(f'{key} {name} count or amplitude')
            if len(w['peaks']) != len(l['peaks']):
                failures.append(f'{key} peak count')
            maxima = []
            for a, b in zip(w['peaks'], l['peaks']):
                if set(a) != {'min', 'max'} or a.keys() != b.keys():
                    raise ValueError(f'Unexpected waveform peak fields for {key}')
                endpoints = [abs(x - y) for x, y in zip(
                    struct.unpack('<ff', struct.pack('<ff', a['min'], a['max'])),
                    struct.unpack('<ff', struct.pack('<ff', b['min'], b['max'])))]
                if not all(map(math.isfinite, endpoints)):
                    raise ValueError(f'Nonfinite waveform peak for {key}')
                maxima.append(max(endpoints))
            report['peaks'] = {'count': len(maxima), 'different_bins': sum(x != 0 for x in maxima),
                               'max_absolute_difference': max(maxima)}
            if max(maxima) > 1e-4:
                failures.append(f'{key} waveform peak amplitude')
    return {'manifest_sha256': linux['manifest_sha256'], 'tracks': tracks, 'failures': failures}
