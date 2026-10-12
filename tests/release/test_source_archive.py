"""Ensure corresponding-source archives preserve patches without local notes."""
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from source_archive import archive_source


class SourceArchive(unittest.TestCase):
    def test_modified_files_and_optional_new_patch_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            subprocess.run(["git", "init", "-q", str(source)], check=True)
            (source / "tracked.c").write_text("original\n")
            (source / ".gitignore").write_text("build/\n")
            subprocess.run(["git", "-C", str(source), "add", "tracked.c", ".gitignore"], check=True)
            (source / "tracked.c").write_text("patched\n")
            (source / "added-test.c").write_text("new test\n")
            (source / "build").mkdir()
            (source / "build/private.log").write_text("not source\n")
            for native in (False, True):
                output = root / f"{native}.tar.gz"
                archive_source(source, output, include_untracked=native)
                with tarfile.open(output) as archive:
                    self.assertEqual(archive.extractfile("source/tracked.c").read(), b"patched\n")
                    self.assertEqual("source/added-test.c" in archive.getnames(), native)
                    self.assertFalse(any(".git/" in name or "private.log" in name for name in archive.getnames()))
                with self.assertRaises(FileExistsError):
                    archive_source(source, output)

    def test_uninitialized_submodule_is_an_error_not_an_incomplete_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            subprocess.run(["git", "init", "-q", str(source)], check=True)
            subprocess.run(["git", "-C", str(source), "update-index", "--add", "--cacheinfo",
                            "160000,cee9b076f2c63104ccfd497fa79c39a867293ec4,dependency"], check=True)
            with self.assertRaisesRegex(ValueError, "Uninitialized submodule"):
                archive_source(source, root / "source.tar.gz")
