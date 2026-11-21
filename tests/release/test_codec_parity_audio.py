"""Only audio amplitudes have a tolerance in the Windows/Linux gate."""
import hashlib
import json
import struct
import tempfile
import unittest
from pathlib import Path

from codec_parity_audio import compare_platform_audio


class CrossPlatformAudio(unittest.TestCase):
    def observations(self, root, sample=0, peak=0.0, cut=2.0, count=1):
        codecs = {}
        for codec in ('vp8', 'vp9'):
            folder = root / codec / 'after'
            folder.mkdir(parents=True)
            waveform = {'sample_count': count, 'peaks': [{'min': peak, 'max': peak}]}
            fields = codecs[codec] = {'cuts': [cut], 'waveform': [waveform, waveform]}
            for track in range(2):
                integer = struct.pack('<h', sample) * count
                fields[f'a:{track}'] = hashlib.sha256(integer).hexdigest()
                for name, data in [('native.s16le', integer), ('native.f32le', struct.pack('<f', sample / 32768) * count),
                                   ('8k.f32le', struct.pack('<f', sample / 32768) * count)]:
                    (folder / f'track-{track}-{name}').write_bytes(data)
        path = root / 'observations.json'
        path.write_text(json.dumps({'codecs': codecs, 'manifest_sha256': 'shared'}))
        return path

    def test_amplitude_boundaries_and_exact_fields(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            windows = self.observations(root / 'windows')
            linux = self.observations(root / 'allowed', sample=2, peak=0.000099)
            report = compare_platform_audio(windows, linux)
            self.assertEqual(report['failures'], [])
            stats = report['tracks']['vp8/a:0']['native.s16le']
            self.assertEqual(stats['first'], {'index': 0, 'windows': 0, 'linux': 2})
            self.assertEqual(stats['different_in_common_prefix'], 1)
            for field, kwargs, failure in [
                ('sample', {'sample': 3}, 'amplitude'), ('peak', {'peak': 0.000101}, 'peak amplitude'),
                ('count', {'count': 2}, 'sample count'), ('cut', {'cut': 2.000001}, 'exact video, timing'),
            ]:
                with self.subTest(field=field):
                    linux = self.observations(root / field, **kwargs)
                    failures = compare_platform_audio(windows, linux)['failures']
                    self.assertTrue(any(failure in message for message in failures))
