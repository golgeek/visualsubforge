"""Archive ANGLE's actual Git checkouts, including nested DEPS repositories.

gclient's CIPD/toolchain binaries are build tools, not Git source checkouts. Their
resolved identities stay in dependency-revisions.txt; this archive contains the
source repositories and the local .gclient recipe, without Git administration.
"""
import json
from pathlib import Path
import re
import subprocess
import tarfile

from native_evidence import digest
from source_archive import tracked_files


def archive_checkouts(root, revisions, destination):
    root = Path(root).resolve(strict=True)
    checkouts = {Path("."): None}
    for line in revisions.splitlines():
        match = re.fullmatch(r"([^:]+): (https://\S+)@([0-9a-f]{40})", line)
        if not match:
            continue
        relative = Path(match[1])
        if relative.is_absolute() or ".." in relative.parts:
            raise ValueError(f"Unsafe gclient checkout path: {relative}")
        checkouts[relative] = match[3]
    paths, records = set(), []
    for relative, expected in sorted(checkouts.items()):
        checkout = root / relative
        if not (checkout / ".git").exists():
            raise ValueError(f"Missing gclient Git checkout: {relative}")
        commit = subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip()
        if expected and commit != expected:
            raise ValueError(f"gclient revision mismatch: {relative}: {commit} != {expected}")
        gitlinks = {}
        files = list(tracked_files(checkout, gitlinks=gitlinks))
        paths.update(relative / file for file in files)
        records.append({"path": relative.as_posix(), "commit": commit, "gitlinks": gitlinks,
                        "files_sha256": {file.as_posix(): digest(checkout / file)
                                         for file in files if not (checkout / file).is_symlink()}})
    if (root / ".gclient").is_file():
        paths.add(Path(".gclient"))
    with tarfile.open(destination, "x:gz", dereference=False) as archive:
        for relative in sorted(paths):
            archive.add(root / relative, arcname=Path("source") / relative, recursive=False)
    manifest = {"schema": 1, "archive_sha256": digest(destination), "checkouts": records,
                "scope": "All gclient-resolved Git source checkouts, including build-only repositories; "
                         "Git link identities are recorded separately because DEPS conditions can disable them"}
    Path(str(destination) + ".json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest
