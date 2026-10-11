#!/usr/bin/env python3
"""Exercise the local continuation property's state and lifecycle contract.

Uses an in-player observer for portability. This is not audio-content validation
and does not execute the application's Rust clear-loop policy.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True, help="Two-second synthetic WAV")
    parser.add_argument("--backend", required=True)
    parser.add_argument("--output-dir", type=Path, required=True, help="New directory")
    args = parser.parse_args()
    player, media = args.mpv.resolve(strict=True), args.reference.resolve(strict=True)
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)
    rows = []
    for speed in (.5, 1, 1.5):
        output = root / f"{speed}.json"
        config = {"output": str(output), "media": str(media), "speed": speed,
                  "backend": args.backend}
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("VSF_", "DYLD_")) and k != "LD_LIBRARY_PATH"}
        env["VSF_CONTINUATION_CONFIG"] = json.dumps(config)
        command = [str(player), "--no-config", "--vo=null", "--vid=no",
                   "--no-audio-display", "--idle=yes", "--pause=yes", "--keep-open=yes",
                   "--hr-seek-demuxer-offset=1", "--ab-loop-count=1000",
                   "--ao=" + args.backend,
                   "--script=" + str(Path(__file__).with_suffix(".lua").resolve())]
        with (root / f"{speed}.log").open("xb") as log:
            process = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
            forced = False
            try:
                code = process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                forced = True
                process.kill()
                code = process.wait()
        trace = json.loads(output.read_text()) if output.exists() else {}
        row = {"speed": speed, "command": command, "exit": code, "forced": forced,
               "binary_sha256": hashlib.sha256(player.read_bytes()).hexdigest(),
               "fixture_sha256": hashlib.sha256(media.read_bytes()).hexdigest(),
               "passed": code == 0 and not forced and trace.get("passed", False)}
        rows.append(row)
        (root / "results.json").write_text(json.dumps(rows, indent=2) + "\n")
        print(json.dumps(row), flush=True)
    return 0 if all(row["passed"] for row in rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
