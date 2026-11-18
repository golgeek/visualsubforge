#!/usr/bin/env python3
"""Fetch pinned ANGLE public headers for adapter compilation, not a runtime SDK."""
import argparse
import hashlib
import gzip
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import time
import urllib.error
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


def download():
    for attempt in range(3):
        try:
            with urllib.request.urlopen(URL, timeout=60) as stream:
                return stream.read(4 * 1024 * 1024 + 1)
        except urllib.error.HTTPError as error:
            if error.code not in (408, 429, 500, 502, 503, 504) or attempt == 2:
                raise
        except (urllib.error.URLError, TimeoutError):
            if attempt == 2:
                raise
        print(f"Transient ANGLE download failure; retrying (attempt {attempt + 2}/3)", flush=True)
        time.sleep(2 ** attempt)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New directory")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--cache-archive", type=Path, help="Read cached archive or save verified download here")
    source.add_argument("--archive", type=Path, help="Use a previously downloaded archive offline")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output already exists; use a new directory")
    archive_path = args.archive or args.cache_archive
    cached = args.archive or (archive_path and archive_path.exists())
    if cached:
        with archive_path.open("rb") as stream:
            data = stream.read(4 * 1024 * 1024 + 1)
    else:
        data = download()
    if len(data) > 4 * 1024 * 1024:
        parser.error("ANGLE header archive exceeds compressed size limit")
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
    if args.cache_archive and not cached:
        args.cache_archive.write_bytes(data)
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
