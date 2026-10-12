import hashlib
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import tarfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location("application_sources", ROOT / "scripts/application-sources.py")
SOURCES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOURCES)


class ApplicationSources(unittest.TestCase):
    def test_installed_crate_must_match_the_source_used_for_redistribution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "lib.rs").write_bytes(b"original source\n")
            archive = root / "source.crate"
            with tarfile.open(archive, "w:gz") as output:
                info = tarfile.TarInfo("example-1.0/lib.rs")
                info.size = len(b"original source\n")
                output.addfile(info, io.BytesIO(b"original source\n"))
            SOURCES.verify_unpacked_crate(archive, root)
            (root / "lib.rs").write_bytes(b"changed compiler input\n")
            with self.assertRaisesRegex(ValueError, "Compiler input differs"):
                SOURCES.verify_unpacked_crate(archive, root)

    def test_checksum_mismatch_cannot_become_a_source_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cached = root / "cached"
            cached.write_bytes(b"the locked source")
            expected = hashlib.sha256(cached.read_bytes()).digest()
            result = SOURCES.verified_download("unused", root / "copied", "sha256", expected, cached)
            self.assertEqual(result["sha256"], expected.hex())
            cached.write_bytes(b"replaced source")
            with self.assertRaisesRegex(ValueError, "Source checksum mismatch"):
                SOURCES.verified_download("unused", root / "rejected", "sha256", expected, cached)
            with self.assertRaisesRegex(ValueError, "Refusing to overwrite"):
                SOURCES.verified_download("unused", root / "copied", "sha256", expected, cached)

    def test_non_dev_closure_keeps_build_inputs_and_shared_normal_edges(self):
        def package(name):
            return {"id": name, "name": name, "source": None if name == "app" else "registry"}
        def edge(name, *kinds):
            return {"pkg": name, "dep_kinds": [{"kind": kind} for kind in kinds]}
        metadata = {"packages": [package(name) for name in ["app", "normal", "build", "dev"]],
                    "resolve": {"nodes": [
                        {"id": "app", "deps": [edge("normal", "dev", None), edge("build", "build"), edge("dev", "dev")]},
                        {"id": "normal", "deps": []}, {"id": "build", "deps": []},
                        {"id": "dev", "deps": []},
                    ]}}
        selected, _ = SOURCES.non_dev_closure(metadata, ["app"])
        self.assertEqual({item["name"] for item in selected}, {"app", "normal", "build"})
        with self.assertRaisesRegex(ValueError, "Missing application package roots"):
            SOURCES.non_dev_closure(metadata, ["missing"])
