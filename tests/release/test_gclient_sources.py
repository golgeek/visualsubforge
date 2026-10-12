from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from gclient_sources import archive_checkouts


class GclientSources(unittest.TestCase):
    def test_nested_deps_are_included_without_build_outputs_or_git_data(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "source"
            def checkout(path, filename):
                path.mkdir(parents=True)
                subprocess.run(["git", "init", "-q", str(path)], check=True)
                (path / filename).write_bytes(b"source\n")
                subprocess.run(["git", "-C", str(path), "add", filename], check=True)
                subprocess.run(["git", "-C", str(path), "-c", "user.name=Fixture", "-c",
                                "user.email=fixture@example.invalid", "commit", "-qm", "fixture"], check=True)
                return subprocess.check_output(["git", "-C", str(path), "rev-parse", "HEAD"], text=True).strip()
            checkout(root, "angle.cc")
            commit = checkout(root / "third_party/libcxx", "LICENSE.TXT")
            # ANGLE declares Git links for both enabled and disabled DEPS.
            # gclient initializes only the resolved set for this build.
            for path in ("third_party/libcxx", "third_party/disabled"):
                subprocess.run(["git", "-C", str(root), "update-index", "--add", "--cacheinfo",
                                f"160000,{commit},{path}"], check=True)
            (root / "private.log").write_text("not part of the source")
            revisions = f"third_party/libcxx: https://example.invalid/libcxx@{commit}\n"
            archive = Path(directory) / "source.tar.gz"
            receipt = archive_checkouts(root, revisions, archive)
            self.assertEqual(len(receipt["checkouts"]), 2)
            self.assertEqual(receipt["checkouts"][0]["gitlinks"]["third_party/disabled"], commit)
            with tarfile.open(archive) as source:
                self.assertEqual(set(source.getnames()), {"source/angle.cc", "source/third_party/libcxx/LICENSE.TXT"})
            bad = revisions.replace(commit, "0" * 40)
            with self.assertRaisesRegex(ValueError, "revision mismatch"):
                archive_checkouts(root, bad, Path(directory) / "bad.tar.gz")
            missing = revisions + f"third_party/disabled: https://example.invalid/disabled@{commit}\n"
            with self.assertRaisesRegex(ValueError, "Missing gclient Git checkout"):
                archive_checkouts(root, missing, Path(directory) / "missing.tar.gz")
