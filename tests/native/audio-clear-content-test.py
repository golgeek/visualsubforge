#!/usr/bin/env python3
"""Synthetic loop-clear preservation, loss, replay and evidence controls."""
import copy
import importlib.util
from pathlib import Path
import unittest

import numpy as np

spec = importlib.util.spec_from_file_location("clear_content", Path(__file__).with_name("audio-clear-content.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ClearControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = np.random.default_rng(505016).uniform(-.2, .2, 72000)
        cls.start, cls.end, cls.target = 12000, 27504, 22000
        cls.before = np.r_[np.tile(cls.source[cls.start:cls.end], 2), cls.source[cls.start:cls.target]]
        cls.command = len(cls.before)

    def verdict(self, captured):
        return probe.analyze(self.source, captured, self.start, self.end, self.command - 32, self.command + 32)

    def test_continuation_and_silent_gap_preserve_source(self):
        for gap in (0, 4800):
            result = self.verdict(np.r_[self.before, np.zeros(gap), self.source[self.target:], np.zeros(1024)])
            self.assertTrue(result["passed"], result)
            if gap:
                self.assertGreater(result["silent_gap_frames"], 4000)

    def test_fractional_resampling_preserves_continuation(self):
        ideal = np.r_[self.before, self.source[self.target:]]
        positions = np.arange(.4, len(ideal) - 1, 1.0001)
        captured = np.interp(positions, np.arange(len(ideal)), ideal)
        command = (self.command - .4) / 1.0001
        result = probe.analyze(self.source, captured, self.start, self.end, command - 32, command + 32)
        self.assertTrue(result["passed"], result)

    def test_loss_is_rejected_with_and_without_silence(self):
        for gap in (0, 2400, 14400):
            result = self.verdict(np.r_[self.before, np.zeros(gap), self.source[self.target + 2400:]])
            self.assertIn("unexplained forward source jump", result["failures"])

    def test_replayed_content_or_another_loop_is_rejected(self):
        for tail in (self.source[self.target - 2400:],
                     np.r_[self.source[self.target:self.end], self.source[self.start:]]):
            result = self.verdict(np.r_[self.before, tail])
            self.assertIn("unexplained repeated source content", result["failures"])

    def test_missing_tail_and_unrelated_interior_do_not_pass(self):
        unrelated = np.random.default_rng(505017).uniform(-.2, .2, 4800)
        for tail in (self.source[self.target:50000],
                     np.r_[unrelated, self.source[self.target + 4800:]]):
            self.assertFalse(self.verdict(np.r_[self.before, tail])["passed"])

    def test_no_anchor_and_unusable_clear_phase_are_rejected(self):
        self.assertFalse(self.verdict(np.zeros(96000))["passed"])
        captured = np.r_[self.before, self.source[self.target:]]
        result = probe.analyze(self.source, captured, self.start, self.end, self.command + 6000, self.command + 6032)
        self.assertIn("pre-command phase does not establish a wrap-free clear", result["failures"])

    def test_invalid_bounds(self):
        for before, after in ((0, 5), (9000, 8000), (9000, float("nan"))):
            with self.assertRaises(ValueError):
                probe.analyze(self.source, self.source, self.start, self.end, before, after)


class CommandControls(unittest.TestCase):
    def test_command_timestamp_conversion_and_rejections(self):
        meta = dict(blocks=[dict(host_ticks=1000000000)], host_time_numer=1, host_time_denom=1)
        trace = dict(capture_exit=0, speed=1, looping=True, start=.75, end=1.073,
                     sample_origin_monotonic_seconds=1.01,
                     transitions=[dict(commands=[["diagnostic-clear-loop-at-audio-pts"]],
                                       before_seconds=4.2, after_seconds=4.201, resolved_seek_seconds=1.0)])
        loop, command = probe.command_bounds(trace, meta)
        self.assertEqual(loop, [36000, 51504])
        self.assertAlmostEqual(command[0], 4.21 * 48000)
        self.assertAlmostEqual(command[1], 4.211 * 48000)
        for field, value in (("capture_exit", 1), ("speed", 1.5), ("looping", False),
                             ("transitions", []), ("start", .750001)):
            broken = copy.deepcopy(trace); broken[field] = value
            with self.assertRaises(ValueError):
                probe.command_bounds(broken, meta)


if __name__ == "__main__":
    unittest.main()
