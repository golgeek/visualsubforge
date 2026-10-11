#!/usr/bin/env python3
"""Synthetic content and timestamp controls; no player or audio device."""
import copy
import importlib.util
from pathlib import Path
import unittest

import numpy as np

spec = importlib.util.spec_from_file_location(
    "fingerprint", Path(__file__).with_name("audio-fingerprint.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ContentControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = np.random.default_rng(505008).uniform(-.2, .2, 48000)

    def verdict(self, captured):
        return probe.analyze(self.source, captured)[0]

    def test_ordinary_offset_and_pause_preserve_content(self):
        for gap in (0, 2400, 14400):
            with self.subTest(gap=gap):
                captured = np.r_[np.zeros(512), self.source[:24000],
                                 np.zeros(gap), self.source[24000:], np.zeros(512)]
                result = self.verdict(captured)
                self.assertTrue(result["passed"], result)
                self.assertLess(result["first_matched_source_frame"], probe.HOP)

    def test_deletion_with_no_short_or_long_capture_gap_is_rejected(self):
        # A short gap would escape the old >100 ms pause-only check. An
        # immediate splice also exercises partially overlapping match windows.
        for gap in (0, 2400, 14400):
            with self.subTest(gap=gap):
                result = self.verdict(np.r_[self.source[:24000],
                                           np.zeros(gap), self.source[26400:]])
                self.assertIn("unexplained forward source jump", result["failures"])

    def test_repeated_source_is_rejected(self):
        result = self.verdict(np.r_[self.source[:24000], self.source[21600:]])
        self.assertIn("unexplained repeated source content", result["failures"])

    def test_silence_unrelated_channel_and_tiny_fragment_do_not_pass(self):
        unrelated = np.random.default_rng(505009).uniform(-.2, .2, 48000)
        for captured in (np.zeros(48000), unrelated,
                         np.r_[self.source[:1024], np.zeros(48000)]):
            with self.subTest(kind=str(captured[:3])):
                self.assertIn("insufficient matched coverage", self.verdict(captured)["failures"])

    def test_missing_edges_are_reported_without_claiming_full_identity(self):
        result = self.verdict(self.source[1024:-1024])
        self.assertTrue(result["passed"])
        self.assertGreaterEqual(result["first_matched_source_frame"], 1024)
        self.assertLessEqual(result["last_matched_source_frame"], 48000 - 1024)

    def test_invalid_samples_are_rejected(self):
        for captured in (np.zeros(128), np.full(48000, np.nan), np.full(48000, np.inf)):
            with self.assertRaises(ValueError):
                self.verdict(captured)

    def test_constant_reference_windows_cannot_match_nonquiet_audio(self):
        old_window, old_hop = probe.WINDOW, probe.HOP
        try:
            probe.WINDOW, probe.HOP = 2048, 256
            for value in (0.0, .125):
                reference = np.r_[self.source[:16384], np.full(28800, value),
                                  self.source[16384:32768]]
                for fractional in (False, True):
                    with self.subTest(value=value, fractional=fractional):
                        rows = probe.match(reference, reference, fractional=fractional)
                        matched = [row for row in rows if row["kind"] == "matched"]
                        self.assertGreater(len(matched), 50)
                        for row in matched:
                            self.assertLessEqual(abs(row["source_frame"] - row["capture_frame"]), 2)
        finally:
            probe.WINDOW, probe.HOP = old_window, old_hop


class TimestampControls(unittest.TestCase):
    def metadata(self):
        return dict(sample_rate=48000, channels=2, frames=1024,
                    host_time_numer=125, host_time_denom=3, blocks=[
                        dict(flags=3, frames=512, first_frame=0,
                             sample_time=20, host_ticks=1000000),
                        dict(flags=3, frames=512, first_frame=512,
                             sample_time=532, host_ticks=1256000)])

    def test_complete_capture(self):
        probe.validate_metadata(self.metadata(), 1024)

    def test_corrupt_timestamps_and_uncovered_pcm_are_rejected(self):
        original = self.metadata()
        for field, value in (("flags", 1), ("frames", 511), ("first_frame", 513),
                             ("sample_time", 533), ("host_ticks", 1257000),
                             ("sample_time", float("nan")),
                             ("host_ticks", float("inf"))):
            meta = copy.deepcopy(original)
            meta["blocks"][1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                probe.validate_metadata(meta, 1024)
        for field, value in (("frames", 1023), ("blocks", []),
                             ("host_time_denom", 0), ("host_time_numer", 0),
                             ("host_time_numer", float("nan"))):
            meta = copy.deepcopy(original)
            meta[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                probe.validate_metadata(meta, 1024)


class FractionalControls(unittest.TestCase):
    def test_known_fractional_offsets_recover_position_without_lower_thresholds(self):
        source = np.random.default_rng(505013).uniform(-.2, .2, 8192)
        for phase in (0, .125, .5, .875):
            positions = np.arange(800, 4800, dtype=float) + phase
            captured = 1.7 * np.interp(positions, np.arange(len(source)), source) + .03
            rows = probe.match(source, captured, fractional=True)
            self.assertTrue(all(row["kind"] == "matched" for row in rows))
            for row in rows:
                self.assertAlmostEqual(row["source_frame"], 800 + phase + row["capture_frame"], places=6)
                self.assertGreater(row["correlation"], .999999)

    def test_unrelated_inverted_and_ambiguous_content_still_do_not_match(self):
        source = np.random.default_rng(505014).uniform(-.2, .2, 8192)
        captured = np.interp(np.arange(800, 4800) + .5, np.arange(len(source)), source)
        for reference, sample in ((source, -captured),
                                  (source, np.random.default_rng(505015).uniform(-.2, .2, 4000)),
                                  (np.tile(source, 2), captured)):
            rows = probe.match(reference, sample, fractional=True)
            self.assertFalse(any(row["kind"] == "matched" for row in rows))

    def test_zero_energy_reference_does_not_divide_by_zero(self):
        with np.errstate(all="raise"):
            rows = probe.match(np.zeros(4096), np.ones(512), fractional=True)
        self.assertFalse(any(row["kind"] == "matched" for row in rows))


if __name__ == "__main__":
    unittest.main()
