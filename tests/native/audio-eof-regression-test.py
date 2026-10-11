#!/usr/bin/env python3
"""Negative controls for paused-reset EOF evidence, without an audio device."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "eof_probe", Path(__file__).with_name("audio-eof-regression.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def trace(mode):
    replacement = mode == "pause-reload"
    return {"backend": "coreaudio", "events": [
        {"kind": "action", "seconds": .95, "position": .95},
        {"kind": "paused-reset", "seconds": .96},
        {"kind": "resume", "seconds": 1.2, "paused": True,
         "source_path": __file__, "transport_position": 0 if replacement else .5,
         "duration": 2 if replacement else 1},
        {"kind": "completed", "seconds": 3.2 if replacement else 1.7},
    ]}


class PausedResetEvidence(unittest.TestCase):
    def result(self, data, mode="pause-reload"):
        return probe.check(data, "coreaudio", mode, .95, __file__)["passed"]

    def test_valid_transitions(self):
        for mode in ("pause-seek", "pause-reload"):
            self.assertTrue(self.result(trace(mode), mode))

    def test_old_media_and_unpaused_state_are_rejected(self):
        for key, value in (("paused", False), ("source_path", probe.__file__),
                           ("duration", 1), ("transport_position", .95)):
            data = trace("pause-reload")
            data["events"][2][key] = value
            with self.subTest(key=key):
                self.assertFalse(self.result(data))

    def test_missing_and_duplicate_reset_are_rejected(self):
        data = trace("pause-reload")
        data["events"].pop(1)
        self.assertFalse(self.result(data))
        data = trace("pause-reload")
        data["events"].append(copy.deepcopy(data["events"][1]))
        self.assertFalse(self.result(data))

    def test_old_eof_deadline_is_rejected(self):
        data = trace("pause-reload")
        data["events"][-1]["seconds"] = 1
        self.assertFalse(self.result(data))

    def test_missing_completion_and_wrong_backend_are_rejected(self):
        data = trace("pause-reload")
        data["events"].pop()
        self.assertFalse(self.result(data))
        data = trace("pause-reload")
        data["backend"] = "null"
        self.assertFalse(self.result(data))


if __name__ == "__main__":
    unittest.main()
