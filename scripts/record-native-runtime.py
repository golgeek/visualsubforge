#!/usr/bin/env python3
"""Record backing-file identities observed in a live, explicitly selected process.

No memory capture or global loader changes. This does not prove the contents of
already-mapped pages or hash macOS images supplied by the system shared cache.
"""
import argparse
import json
from pathlib import Path
import platform
import re
import subprocess
from datetime import datetime, timezone
from native_evidence import file_identity


def is_image(path, expected):
    file = Path(path)
    return (file.resolve() in expected
            or bool(re.search(r"\.(?:dylib|dll|exe|so(?:\.\d+)*)$", path, re.IGNORECASE))
            or any(part.endswith(".framework") and file.name == part[:-10]
                   for part in file.parts))


def capture(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=60).strip()


def process_snapshot(pid):
    host = platform.system()
    if host == "Darwin":
        start = capture("ps", "-p", str(pid), "-o", "lstart=", "-o", "comm=")
        output = capture("lsof", "-a", "-p", str(pid), "-d", "txt", "-Fn")
        paths = [line[1:] for line in output.splitlines() if line.startswith("n/")]
    elif host == "Linux":
        root = Path("/proc") / str(pid)
        # Fields after the final ')' begin with field 3, the process state.
        start = (root / "stat").read_text().rsplit(")", 1)[1].split()[19]
        paths = [line.split(maxsplit=5)[5] for line in (root / "maps").read_text().splitlines()
                 if len(line.split(maxsplit=5)) == 6 and line.split(maxsplit=5)[5].startswith("/")]
    elif host == "Windows":
        # pid is parsed as a positive integer, never interpolated free text.
        script = (f"$ErrorActionPreference='Stop'; $p=Get-Process -Id {pid}; "
                  "@{start=$p.StartTime.ToUniversalTime().ToString('o'); "
                  "paths=@($p.Modules | ForEach-Object {$_.FileName})} | ConvertTo-Json -Compress")
        data = json.loads(capture("powershell", "-NoProfile", "-Command", script))
        start, paths = data["start"], data["paths"]
    else:
        raise ValueError(f"Unsupported process inspection host: {host}")
    if not start or not paths:
        raise ValueError("Process exited or no mapped images could be inspected")
    return start, sorted(set(paths))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--expect", type=Path, action="append", required=True,
                        help="Required loaded file; repeat for executable, libmpv and ANGLE")
    parser.add_argument("--output", type=Path, required=True, help="New JSON receipt")
    args = parser.parse_args()
    if args.pid <= 0 or args.output.exists():
        parser.error("Use a positive PID and a new output filename")
    expected = [file_identity(path) for path in args.expect]
    expected_paths = {Path(item["path"]) for item in expected}
    start, paths = process_snapshot(args.pid)
    images, unavailable = [], []
    for path in paths:
        # lsof/proc maps also expose fonts and unrelated mapped data caches.
        # Retain only library images and explicitly requested executables.
        if not is_image(path, expected_paths):
            continue
        try:
            images.append(file_identity(path))
        except (OSError, ValueError) as error:
            unavailable.append({"path": path, "error": str(error)})
    end, _ = process_snapshot(args.pid)
    if end != start:
        raise ValueError("Process identity changed during inspection")
    missing = [item for item in expected if item not in images]
    receipt = {
        "schema": 1, "status": "failed" if missing else "recorded",
        "pid": args.pid, "process_start": start,
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "host": {"system": platform.system(), "release": platform.release(), "machine": platform.machine()},
        "scope": "Observed mapped files hashed from disk; no mapped-memory or device validation",
        "tools": [file_identity(Path(__file__)),
                  file_identity(Path(__file__).with_name("native_evidence.py"))],
        "expected": expected, "missing_expected": missing,
        "images": images, "unavailable_images": unavailable,
    }
    with args.output.open("x") as stream:
        json.dump(receipt, stream, indent=2)
        stream.write("\n")
    if missing:
        raise ValueError(f"Expected runtime files were not observed; see {args.output}")
    print(f"RECORDED: {len(images)} images, {len(unavailable)} unavailable; {args.output}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise SystemExit(f"Native runtime recording failed: {error}") from error
