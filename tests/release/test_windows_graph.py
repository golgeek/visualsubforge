"""Dependency safety controls independent of a Windows runner."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location("package_windows", ROOT / "scripts/package-windows.py")
PACKAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGER)


class WindowsGraph(unittest.TestCase):
    def test_private_graph_rejects_missing_ambiguous_and_hazardous_dependencies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            app, library = root / "vsf.exe", root / "media.dll"
            app.touch()
            library.touch()
            imports = {app: ["media.dll", "kernel32.dll"], library: ["ucrtbase.dll"]}
            graph, _ = PACKAGER.resolve_graph([app], [root], imports.__getitem__)
            self.assertEqual(set(graph), {"vsf.exe", "media.dll"})
            other = root / "other"
            other.mkdir()
            (other / library.name).touch()
            with self.assertRaisesRegex(ValueError, "ambiguous DLL"):
                PACKAGER.resolve_graph([app], [root, other], imports.__getitem__)
            library.unlink()
            with self.assertRaisesRegex(ValueError, "Missing"):
                PACKAGER.resolve_graph([app], [root], imports.__getitem__)
            for dependency in ("ggml-base-0.dll", "libwhisper.dll", "msys-2.0.dll"):
                imports[app] = [dependency]
                with self.assertRaisesRegex(ValueError, "Unexpected runtime"):
                    PACKAGER.resolve_graph([app], [root], imports.__getitem__)

    def test_compiler_runtime_is_not_assumed_to_be_an_os_library(self):
        self.assertFalse(PACKAGER.is_platform("vcruntime140.dll"))
        self.assertFalse(PACKAGER.is_platform("libc++.dll"))
        self.assertTrue(PACKAGER.is_platform("api-ms-win-crt-runtime-l1-1-0.dll"))
        self.assertTrue(PACKAGER.is_platform("avicap32.dll"))
        self.assertTrue(PACKAGER.is_platform("bcryptprimitives.dll"))

    def test_one_failed_audit_reports_all_unresolved_imports(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "vsf.exe"
            app.touch()
            with self.assertRaises(ValueError) as error:
                PACKAGER.resolve_graph([app], [root], lambda _: ["missing-a.dll", "missing-b.dll"])
            self.assertIn("missing-a.dll", str(error.exception))
            self.assertIn("missing-b.dll", str(error.exception))
