"""Only the 6.1.1 Matroska Opus origin shift can satisfy the exception."""
from copy import deepcopy
from pathlib import Path
import runpy
import unittest

matches = runpy.run_path(str(Path(__file__).with_name('codec-parity.py')))['ubuntu_timing_matches']


class UbuntuTiming(unittest.TestCase):
    def test_rejects_additional_frame_sample_and_srt_changes(self):
        wave = {'sample_count': 32000, 'container_origin_seconds': 0, 'samples_per_peak': 80,
                'sample_rate': 8000, 'audio_stream_index': 2}
        old_wave = {**wave, 'sample_count': 32056, 'container_origin_seconds': -0.007}
        after = {'cuts': [2.0], 'waveform': [wave], 'frames': [{'pts_us': 2000000, 'sha256': 'matched'}],
                 'subtitles': {'offset': 'matched'}}
        report = {'observations': {'before': {**deepcopy(after), 'cuts': [2.007], 'waveform': [old_wave]},
                                   'after': after},
                  'ffmpeg_versions': ['ffmpeg version 6.1.1-Ubuntu', 'ffmpeg version 8.1.2'],
                  'waveform_alignment': [{'offset_samples': 56, 'first_samples_silent': True,
                                          'aligned_samples_identical': True}]}
        self.assertTrue(matches(report))
        for field, value in [('frames', [{'pts_us': 1999999, 'sha256': 'matched'}]),
                             ('frames', [{'pts_us': 2000000, 'sha256': 'different'}]),
                             ('cuts', [2.001]), ('subtitles', {'offset': 'different'})]:
            changed = deepcopy(report)
            changed['observations']['after'][field] = value
            self.assertFalse(matches(changed))
        for field, value in [('offset_samples', 55), ('first_samples_silent', False),
                             ('aligned_samples_identical', False)]:
            changed = deepcopy(report)
            changed['waveform_alignment'][0][field] = value
            self.assertFalse(matches(changed))
