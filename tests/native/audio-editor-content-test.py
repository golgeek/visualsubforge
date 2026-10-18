#!/usr/bin/env python3
"""Controls for editor suffix start, EOF and corruption rejection."""
import importlib.util
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


if __name__ == "__main__":
    unittest.main()
