#!/usr/bin/env python3
"""Fetch pinned ANGLE public headers for adapter compilation, not a runtime SDK."""
import argparse
import hashlib
import gzip
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import urllib.request

PIN = json.loads((Path(__file__).resolve().parent.parent / "tests/native/angle/manifest.json").read_text())
COMMIT = PIN["upstream_commit"]
URL = f"https://chromium.googlesource.com/angle/angle/+archive/{COMMIT}/include.tar.gz"
TREE_SHA256 = PIN["headers_tree_sha256"]


def contents(archive):
    records = []
    seen = set()
    for member in archive:
        path = PurePosixPath(member.name)
        if (path.is_absolute() or ".." in path.parts or member.name in seen
                or not (member.isfile() or member.isdir())):
            raise ValueError(f"Unexpected ANGLE archive member: {member.name}")
        seen.add(member.name)
        records.append({"path": member.name, "kind": "file" if member.isfile() else "directory",
                        "sha256": hashlib.sha256(archive.extractfile(member).read()).hexdigest()
                        if member.isfile() else None})
    records.sort(key=lambda entry: entry["path"])
    return hashlib.sha256(json.dumps(records, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New directory")
    parser.add_argument("--archive", type=Path, help="Use a previously downloaded archive offline")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output already exists; use a new directory")
    if args.archive:
        with args.archive.open("rb") as stream:
            data = stream.read(4 * 1024 * 1024 + 1)
    else:
        with urllib.request.urlopen(URL, timeout=60) as stream:
            data = stream.read(4 * 1024 * 1024 + 1)
    # Gitiles regenerates tar timestamps on download. Pin names, kinds and file
    # bytes, not that variable transport metadata. Bound decompression first.
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as stream:
        unpacked = stream.read(8 * 1024 * 1024 + 1)
    if len(unpacked) > 8 * 1024 * 1024:
        parser.error("ANGLE header archive exceeds decompressed size limit")
    with tarfile.open(fileobj=io.BytesIO(unpacked), mode="r:") as archive:
        if contents(archive) != TREE_SHA256:
            parser.error("ANGLE header contents checksum mismatch; no files extracted")
        args.output.mkdir(parents=True, exist_ok=False)
        archive.extractall(args.output / "include", filter="data")
    (args.output / "headers-manifest.json").write_text(json.dumps({
        "source_commit": COMMIT, "archive_url": URL, "contents_sha256": TREE_SHA256,
        "download_sha256": hashlib.sha256(data).hexdigest(),
        "scope": "Public headers only; no libraries, link, device or runtime acceptance",
    }, indent=2) + "\n")
    print(args.output.resolve())


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, tarfile.TarError) as error:
        raise SystemExit(f"Unable to prepare ANGLE headers: {error}") from error
