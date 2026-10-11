#!/usr/bin/env python3
"""Positive and deliberately corrupted steady-loop content controls."""
import importlib.util
from pathlib import Path
import unittest

import numpy as np

spec = importlib.util.spec_from_file_location("loop_content", Path(__file__).with_name("audio-loop-content.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class LoopControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = np.random.default_rng(505010).uniform(-.2, .2, 72000)
        cls.start, cls.end = 36000, 51504
        cls.loop = cls.source[cls.start:cls.end]

    def verdict(self, captured):
        return probe.analyze(self.source, captured, self.start, self.end)

    def test_correct_loop_with_partial_edges(self):
        result = self.verdict(np.tile(self.loop, 5)[997:-1193])
        self.assertTrue(result["passed"], result)
        self.assertEqual(len(result["wrap_capture_frames"]), 4)

    def test_overshoot_and_undershoot(self):
        for a, b in ((self.start - 1300, self.end), (self.start, self.end + 1300)):
            result = self.verdict(np.tile(self.source[a:b], 5))
            self.assertIn("matched content outside loop range", result["failures"])

    def test_early_wrap_is_not_normalized(self):
        result = self.verdict(np.tile(self.loop[:-2400], 5))
        self.assertFalse(result["passed"])
        self.assertEqual(result["wrap_capture_frames"], [])

    def test_interior_loss_and_repeat(self):
        for damaged in (np.r_[self.loop[:7000], self.loop[9400:]],
                        np.r_[self.loop[:9400], self.loop[7000:]]):
            result = self.verdict(np.tile(damaged, 5))
            self.assertFalse(result["passed"])
            self.assertTrue(any("source" in f for f in result["failures"]))

    def test_silent_or_unrelated_interval_cannot_hide_a_missing_iteration(self):
        for gap in (np.zeros(len(self.loop)), np.random.default_rng(505011).uniform(-.2, .2, len(self.loop))):
            result = self.verdict(np.r_[np.tile(self.loop, 3), gap, np.tile(self.loop, 3)])
            self.assertIn("unverified interior capture interval", result["failures"])

    def test_silence_ordinary_playback_and_one_wrap_are_insufficient(self):
        for captured in (np.zeros(72000), self.source, np.tile(self.loop, 2)):
            self.assertFalse(self.verdict(captured)["passed"])

    def test_obscured_valid_boundary_is_unverified_not_proven_replay(self):
        for radius in (1200, 5000):
            captured = np.tile(self.loop, 5)
            boundary = 2 * len(self.loop)
            captured[boundary - radius:boundary + radius] = np.random.default_rng(505012).uniform(-.2, .2, 2 * radius)
            result = self.verdict(captured)
            self.assertIn("unverified loop boundary", result["failures"])
            self.assertNotIn("unexplained repeated source content", result["failures"])

    def test_invalid_bounds(self):
        for start, end in ((-1, 5000), (0, 1024), (5000, 5000), (0, 72001)):
            with self.assertRaises(ValueError):
                probe.analyze(self.source, self.loop, start, end)


class FractionalLoopControls(LoopControls):
    def verdict(self, captured):
        return probe.analyze(self.source, captured, self.start, self.end, fractional=True)

    def test_fractional_phase_and_small_rate_difference_preserve_loop_content(self):
        ideal = np.tile(self.loop, 5)
        positions = np.arange(997.4, len(ideal) - 1193, 1.0001)
        captured = np.interp(positions, np.arange(len(ideal)), ideal)
        result = self.verdict(captured)
        self.assertTrue(result["passed"], result)
        self.assertEqual(len(result["wrap_capture_frames"]), 4)

    def test_resampling_does_not_hide_loss_replay_or_range_leakage(self):
        for loop in (np.r_[self.loop[:7000], self.loop[9400:]],
                     np.r_[self.loop[:9400], self.loop[7000:]],
                     self.source[self.start:self.end + 1300]):
            ideal = np.tile(loop, 5)
            positions = np.arange(997.4, len(ideal) - 1193, 1.0001)
            captured = np.interp(positions, np.arange(len(ideal)), ideal)
            self.assertFalse(self.verdict(captured)["passed"])


if __name__ == "__main__":
    unittest.main()
