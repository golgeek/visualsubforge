#!/usr/bin/env python3
"""Negative controls for the tail regression's content/duration contract."""
import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location(
    "stretch", Path(__file__).with_name("audio-stretch-eof-regression.py"))
stretch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stretch)


def pcm(values):
    return b"".join(struct.pack("<hh", n, -n) for n in values)


class TailContract(unittest.TestCase):
    def setUp(self):
        self.original = pcm(range(1, 7))
        self.complete = pcm(range(1, 11))

    def check(self, candidate, reference=None):
        return stretch.compare(self.original, candidate,
                               self.complete if reference is None else reference, 10, 1)["passed"]

    def test_real_tail_passes(self):
        self.assertTrue(self.check(self.complete))

    def test_zero_padding_is_not_tail_content(self):
        self.assertFalse(self.check(self.original + bytes(16)))

    def test_missing_tail_fails(self):
        self.assertFalse(self.check(self.original))

    def test_repeated_tail_fails(self):
        self.assertFalse(self.check(self.original + pcm([6, 6, 6, 6])))

    def test_changed_old_sample_fails_even_when_reference_agrees(self):
        altered = pcm([2, *range(2, 11)])
        self.assertFalse(self.check(altered, altered))

    def test_unchanged_output_does_not_need_extended_input_rounding_identity(self):
        reference = pcm([*range(1, 10), 11])
        result = stretch.compare(self.complete, self.complete, reference, 10, 1)
        self.assertTrue(result["passed"])
        self.assertFalse(result["candidate_matches_reference"])

    def test_partial_stereo_frame_fails(self):
        self.assertFalse(self.check(self.complete + b"\0", self.complete + b"\0"))


if __name__ == "__main__":
    unittest.main()
