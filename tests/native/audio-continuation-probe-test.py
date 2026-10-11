#!/usr/bin/env python3
"""Regression checks for continuation-gap analysis; no player or device needed."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    "continuation", Path(__file__).with_name("audio-continuation-probe.py"))
continuation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(continuation)


class ContinuationAnalysisTests(unittest.TestCase):
    def measure(self, observations):
        windows = [{"seconds": time, "rms": rms, "frequency_hz": None}
                   for time, rms in observations]
        trace = {"case": "synthetic", "transitions": [{
            "before_seconds": 1.0, "after_seconds": 1.02,
            "resolved_seek_seconds": 1.1}], "samples": []}
        with patch.object(continuation, "capture_windows", return_value=(windows, 0.002)):
            return continuation.measure(trace)

    def test_interruption_during_clear_is_not_skipped(self):
        # The command returns only after audio resumes. Bounding analysis by
        # the first post-command sound would incorrectly report no gap.
        result = self.measure([(0.999, 0.2), (1.001, 0), (1.003, 0),
                               (1.005, 0.2), (1.021, 0.2)])
        self.assertTrue(result["gap_analysis_has_pre_clear_audio"])
        self.assertEqual(len(result["silent_intervals_seconds"]), 1)
        start, end = result["silent_intervals_seconds"][0]
        self.assertAlmostEqual(start, 1.0)
        self.assertAlmostEqual(end, 1.004)

    def test_quiet_eof_tail_is_not_a_restart_gap(self):
        result = self.measure([(0.999, 0.2), (1.021, 0.2), (2.001, 0.2),
                               (2.003, 0), (2.005, 0)])
        self.assertEqual(result["silent_intervals_seconds"], [])

    def test_missing_pre_clear_sound_is_explicit(self):
        result = self.measure([(0.999, 0), (1.001, 0), (1.021, 0.2)])
        self.assertFalse(result["gap_analysis_has_pre_clear_audio"])


if __name__ == "__main__":
    unittest.main()
