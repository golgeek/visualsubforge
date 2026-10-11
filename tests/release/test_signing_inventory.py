"""Signing must reject code that differs from the tested assembly artifact."""
import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("sign_macos", ROOT / "scripts/sign-macos.py")
SIGNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SIGNER)


class SigningInventory(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.app = Path(self.temporary.name).resolve() / "VSF.app"
        binaries = self.app / "Contents/MacOS"
        binaries.mkdir(parents=True)
        self.manifest = {"files": []}
        for name in ("visualsubforge", "vsf", "ffmpeg", "ffprobe"):
            path = binaries / name
            path.write_bytes(bytes.fromhex("cffaedfe") + b"fixture")
            self.manifest["files"].append({"file": str(path.relative_to(self.app)),
                                           "bundled_sha256": SIGNER.digest(path)})

    def test_complete_unchanged_inventory(self):
        self.assertEqual(len(SIGNER.validated_files(self.app, self.manifest)), 4)

    def test_final_receipt_also_protects_non_native_resources(self):
        resource = self.app / "Contents/Resources"
        resource.mkdir()
        script = resource / "app.js"
        script.write_text("original")
        receipt = {"bundle_files_sha256": {
            str(path.relative_to(self.app)): SIGNER.digest(path)
            for path in self.app.rglob("*") if path.is_file()
        }}
        SIGNER.validate_bundle_hashes(self.app, receipt)
        script.write_text("modified")
        with self.assertRaisesRegex(ValueError, "differ"):
            SIGNER.validate_bundle_hashes(self.app, receipt)

    def test_changed_and_missing_tools_are_rejected(self):
        tool = self.app / "Contents/MacOS/ffprobe"
        tool.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "differs"):
            SIGNER.validated_files(self.app, self.manifest)
        self.manifest["files"].pop()
        with self.assertRaisesRegex(ValueError, "Incomplete"):
            SIGNER.validated_files(self.app, self.manifest)

    def test_uninventoried_native_code_is_rejected(self):
        resource = self.app / "Contents/Resources"
        resource.mkdir()
        (resource / "hidden.dylib").write_bytes(bytes.fromhex("cffaedfe") + b"extra")
        with self.assertRaisesRegex(ValueError, "Uninventoried"):
            SIGNER.validated_files(self.app, self.manifest)

    def test_escape_and_duplicate_records_are_rejected(self):
        original = self.manifest["files"][0]["file"]
        for path in ("../outside", "/absolute/file", "Contents/Resources/tool"):
            self.manifest["files"][0]["file"] = path
            with self.subTest(path=path), self.assertRaises(ValueError):
                SIGNER.validated_files(self.app, self.manifest)
        self.manifest["files"][0]["file"] = original
        self.manifest["files"].append(dict(self.manifest["files"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            SIGNER.validated_files(self.app, self.manifest)


if __name__ == "__main__":
    unittest.main()
