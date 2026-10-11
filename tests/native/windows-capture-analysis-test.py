#!/usr/bin/env python3
"""Clock-alignment invariants; no Windows device is required for these checks."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'capture_analysis', Path(__file__).with_name('windows-capture-analysis.py'))
analysis = importlib.util.module_from_spec(spec)
spec.loader.exec_module(analysis)


def bracket(phase, before, after, mp_ns):
    return (f'VSF_CLOCK_BRACKET phase={phase} qpc_before={before} '
            f'qpc_after={after} frequency=10000000 mp_ns={mp_ns}')


class ClockBrackets(unittest.TestCase):
    def test_intersection_preserves_uncertainty(self):
        # Both clocks share a 100 s offset. The second measurement is narrower.
        rows = [bracket(0, 1009999990, 1010000010, 1000000000),
                bracket(1, 1019999998, 1020000002, 2000000000)]
        low, high, count = analysis.native_clock_interval('\n'.join(rows))
        self.assertEqual(count, 2)
        self.assertLess(low, 100)
        self.assertGreater(high, 100)
        self.assertAlmostEqual(high - low, .000000402, places=11)

    def test_rejects_drift_instead_of_fitting_it(self):
        rows = [bracket(0, 1010000000, 1010000001, 1000000000),
                bracket(1, 1020010000, 1020010001, 2000000000)]
        with self.assertRaisesRegex(ValueError, 'disagree'):
            analysis.native_clock_interval('\n'.join(rows))

    def test_requires_both_lifecycle_ends(self):
        row = bracket(0, 1010000000, 1010000001, 1000000000)
        with self.assertRaisesRegex(ValueError, 'initialization and teardown'):
            analysis.native_clock_interval(row + '\n' + row)

    def test_rejects_reversed_bracket(self):
        with self.assertRaisesRegex(ValueError, 'invalid native clock'):
            analysis.native_clock_interval(bracket(0, 20, 10, 100))


if __name__ == '__main__':
    unittest.main()
