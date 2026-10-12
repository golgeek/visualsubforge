"""Archive the actual Git source tree, including patches and pinned submodules.

Build directories belong outside the source tree. By default only tracked files
are copied, excluding local notes, credentials and Git administration data.
Fresh native checkouts may explicitly include patch-created untracked files.
"""
from pathlib import Path
import subprocess
import tarfile


def tracked_files(root, include_untracked=False):
    entries = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=root)
    for entry in filter(None, entries.split(b"\0")):
        metadata, name = entry.split(b"\t", 1)
        mode, _, stage = metadata.split()
        if stage != b"0":
            raise ValueError("Cannot archive an unresolved source tree")
        path = Path(name.decode("utf-8"))
        if mode == b"160000":
            child = root / path
            if not (child / ".git").exists():
                raise ValueError(f"Uninitialized submodule: {child}")
            for nested in tracked_files(child, include_untracked):
                yield path / nested
        else:
            yield path
    if include_untracked:
        entries = subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=root)
        for name in filter(None, entries.split(b"\0")):
            yield Path(name.decode("utf-8"))


def archive_source(root, destination, include_untracked=False):
    root = Path(root).resolve(strict=True)
    with tarfile.open(destination, "x:gz", dereference=False) as archive:
        for relative in sorted(tracked_files(root, include_untracked)):
            archive.add(root / relative, arcname=Path("source") / relative, recursive=False)
