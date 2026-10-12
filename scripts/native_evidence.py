"""File identity shared by native build, SDK and live-process evidence tools."""
import hashlib
import json
from pathlib import Path


def file_identity(path):
    path = Path(path).resolve(strict=True)
    before = path.stat()
    if not path.is_file():
        raise ValueError(f"Expected a regular file: {path}")
    with path.open("rb") as stream:
        sha = hashlib.file_digest(stream, "sha256").hexdigest()
    after = path.stat()
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise ValueError(f"File changed while hashing: {path}")
    return {"path": str(path), "bytes": after.st_size, "sha256": sha}


def digest(path):
    return file_identity(path)["sha256"]


def verified_headers(receipt_path):
    """Validate embedded headers whose packages are absent from dynamic links."""
    receipt = json.loads(Path(receipt_path).read_text(encoding="utf-8"))
    if receipt["status"] != "passed":
        raise ValueError("Header inventory requires a successful native build")
    headers = receipt["header_inputs"]
    for header in headers:
        if digest(header["path"]) != header["sha256"]:
            raise ValueError(f'Header changed since native compilation: {header["path"]}')
    return headers
