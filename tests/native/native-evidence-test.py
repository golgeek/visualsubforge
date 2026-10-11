"""Controls for native evidence boundaries, without a device or live process."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))
from native_evidence import file_identity

spec = importlib.util.spec_from_file_location("runtime", SCRIPTS / "record-native-runtime.py")
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


class EvidenceTests(unittest.TestCase):
    def test_only_expected_executable_and_library_images_are_selected(self):
        expected = {Path("/tmp/editor").resolve()}
        for path in ["/tmp/editor", "/lib/libmpv.so.2.5.0", "/tmp/libEGL.dylib",
                     "/tmp/mpv-2.dll", "/tmp/editor.exe",
                     "/System/Library/Frameworks/AppKit.framework/Versions/C/AppKit"]:
            with self.subTest(path=path):
                self.assertTrue(runtime.is_image(path, expected))
        for path in ["/private/var/db/analyticsd/events.allowlist", "/tmp/cache.csstore",
                     "/System/Library/Frameworks/AppKit.framework/Resources/Info.plist",
                     "/tmp/font.ttf", "/tmp/libmpv.so.2 (deleted)"]:
            with self.subTest(path=path):
                self.assertFalse(runtime.is_image(path, expected))

    def test_identity_changes_with_bytes_and_rejects_missing_or_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "libmpv"
            path.write_bytes(b"one")
            before = file_identity(path)
            path.write_bytes(b"two")
            self.assertNotEqual(before["sha256"], file_identity(path)["sha256"])
            with self.assertRaises(ValueError):
                file_identity(Path(directory))
            with self.assertRaises(OSError):
                file_identity(Path(directory) / "missing")


if __name__ == "__main__":
    unittest.main()
