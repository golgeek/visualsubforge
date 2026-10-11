"""Pinned header identity excludes timestamps, never file contents or unsafe members."""
import importlib.util
import io
from pathlib import Path
import tarfile
import unittest

spec = importlib.util.spec_from_file_location(
    "angle_headers", Path(__file__).resolve().parents[2] / "scripts/prepare-angle-headers.py"
)
headers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(headers)


def tree(name="EGL/egl.h", data=b"header", timestamp=1, kind=tarfile.REGTYPE):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as archive:
        entry = tarfile.TarInfo(name)
        entry.mtime = timestamp
        entry.type = kind
        entry.size = len(data) if kind == tarfile.REGTYPE else 0
        archive.addfile(entry, io.BytesIO(data))
    buffer.seek(0)
    return tarfile.open(fileobj=buffer, mode="r:")


class HeaderTests(unittest.TestCase):
    def test_generated_archive_timestamps_do_not_change_source_identity(self):
        with tree(timestamp=1) as before, tree(timestamp=2) as after:
            self.assertEqual(headers.contents(before), headers.contents(after))

    def test_changed_bytes_or_filename_change_identity(self):
        with tree() as original:
            expected = headers.contents(original)
        for changed in [tree(data=b"modified"), tree(name="EGL/other.h")]:
            with changed:
                self.assertNotEqual(expected, headers.contents(changed))

    def test_path_escape_or_link_is_rejected(self):
        for unsafe in [tree(name="../../outside"), tree(name="/absolute"), tree(kind=tarfile.SYMTYPE)]:
            with unsafe, self.assertRaises(ValueError):
                headers.contents(unsafe)


if __name__ == "__main__":
    unittest.main()
