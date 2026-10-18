#!/usr/bin/env python3
"""Controls for editor suffix start, EOF and corruption rejection."""
import importlib.util
import copy
from pathlib import Path
import unittest

import numpy as np

spec = importlib.util.spec_from_file_location("editor_content", Path(__file__).with_name("audio-editor-content.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class SuffixControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = np.random.default_rng(505021).uniform(-.2, .2, 72000)
        cls.start = 12000

    def check(self, captured):
        return probe.suffix(self.source, captured, self.start)

    def test_requested_suffix_with_capture_silence(self):
        result = self.check(np.r_[np.zeros(1000), self.source[self.start:], np.zeros(5000)])
        self.assertTrue(result["passed"], result)

    def test_wrong_start_or_missing_eof_is_rejected(self):
        for data in (self.source[self.start + 2400:], self.source[self.start:68000]):
            self.assertFalse(self.check(data)["passed"])

    def test_missing_and_repeated_interior_are_rejected(self):
        for data in (np.r_[self.source[self.start:36000], self.source[38400:]],
                     np.r_[self.source[self.start:38400], self.source[36000:]]):
            self.assertFalse(self.check(data)["passed"])

    def test_wrong_replacement_audio_and_empty_capture_are_rejected(self):
        wrong = np.random.default_rng(505022).uniform(-.2, .2, 72000)
        for data in (wrong, np.zeros(72000)):
            self.assertFalse(self.check(data)["passed"])

    def test_uninterrupted_suffix_rejects_inserted_silence(self):
        result = self.check(np.r_[self.source[self.start:36000], np.zeros(2400), self.source[36000:]])
        self.assertIn("unexpected interior silence", result["failures"])


class PulseMetadataControls(unittest.TestCase):
    def setUp(self):
        self.metadata = dict(capture_kind='pulse-single-stream-receive-time',
            player_pid=42, sample_rate=48000, channels=2, format='float32le',
            frames=8, recorder_exit=0, blocks=[
                dict(byte_offset=0, bytes=32, received_monotonic_seconds=1.0),
                dict(byte_offset=32, bytes=32, received_monotonic_seconds=1.1)])

    def validate(self, metadata):
        probe.validate_capture(metadata, 8, 42, 'pulse-single-stream-receive-time')

    def test_complete_single_process_capture(self):
        self.validate(self.metadata)

    def test_pipe_reads_may_split_a_complete_pcm_frame(self):
        self.metadata['blocks'][0]['bytes'] = 31
        self.metadata['blocks'][1].update(byte_offset=31, bytes=33)
        self.validate(self.metadata)

    def test_wrong_identity_format_or_failed_recorder(self):
        for key, value in [('capture_kind', 'whole-sink'), ('player_pid', 43),
                           ('channels', 1), ('sample_rate', 44100),
                           ('format', 's16le'), ('frames', 9), ('recorder_exit', 1)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.validate({**self.metadata, key: value})

    def test_missing_reordered_or_malformed_blocks(self):
        for key, value in [('byte_offset', 0), ('bytes', 31), ('bytes', 0),
                           ('received_monotonic_seconds', .9),
                           ('received_monotonic_seconds', float('nan'))]:
            metadata = copy.deepcopy(self.metadata)
            metadata['blocks'][1][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                self.validate(metadata)
        with self.assertRaises(ValueError):
            self.validate({**self.metadata, 'blocks': self.metadata['blocks'][:1]})

    def test_unknown_capture_kind(self):
        with self.assertRaises(ValueError):
            probe.validate_capture(self.metadata, 8, 42, 'unidentified')


if __name__ == "__main__":
    unittest.main()
