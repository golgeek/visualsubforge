"""Negative controls for the native build receipt, without building mpv."""
import importlib.util
from pathlib import Path
import unittest
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))

spec = importlib.util.spec_from_file_location(
    "mpv_build", Path(__file__).resolve().parents[2] / "scripts/check-mpv-build.py"
)
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)


class ReceiptTests(unittest.TestCase):
    def test_only_requested_binaries_are_required_and_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            exe, dll = root / "mpv.exe", root / "mpv-2.dll"
            exe.write_bytes(b"built executable")
            dll.write_bytes(b"built library")
            targets = [
                {"name": "mpv", "type": "executable", "filename": [str(exe)]},
                {"name": "mpv", "type": "shared library", "filename": [str(dll)]},
                {"name": "mpv", "type": "executable", "filename": [str(root / "mpv.com")]},
            ]
            receipt = build.binary_receipts(targets)
            self.assertEqual({item["path"] for item in receipt}, {str(exe), str(dll)})
            with self.assertRaisesRegex(ValueError, "Missing selected"):
                build.binary_receipts(targets[:1])
            dll.unlink()
            with self.assertRaises(FileNotFoundError):
                build.binary_receipts(targets)

    def setUp(self):
        self.declared = [{"name": name} for name in ["libmpv-lifetime", "queued-pts", "aframe-clip"]]
        self.results = [
            {"name": "libmpv - mpv:libmpv-lifetime", "result": "OK"},
            {"name": "mpv:queued-pts", "result": "OK"},
            {"name": "mpv:aframe-clip", "result": "OK"},
        ]

    def test_executed_required_tests_accept_meson_suite_names(self):
        self.assertEqual(build.validate_results(self.declared, self.results, "candidates"),
                         {"OK": 3, "SKIP": 0})

    def test_empty_missing_duplicate_unknown_or_failed_results_reject(self):
        for results in [[], self.results[:-1], self.results + self.results[:1],
                        self.results + [{"name": "mpv:unexpected", "result": "OK"}],
                        [dict(test, result="FAIL") for test in self.results]]:
            with self.subTest(results=results), self.assertRaises(ValueError):
                build.validate_results(self.declared, results, "candidates")

    def test_audio_variant_cannot_pass_without_audio_tests_declared(self):
        with self.assertRaises(ValueError):
            build.validate_results(self.declared[:1], self.results[:1], "baseline")
        self.assertEqual(build.validate_results(self.declared[:1], self.results[:1], "render"),
                         {"OK": 1, "SKIP": 0})

    def test_required_tests_may_not_skip(self):
        for index in range(len(self.results)):
            results = [test.copy() for test in self.results]
            results[index]["result"] = "SKIP"
            with self.subTest(index=index), self.assertRaises(ValueError):
                build.validate_results(self.declared, results, "candidates")


if __name__ == "__main__":
    unittest.main()
