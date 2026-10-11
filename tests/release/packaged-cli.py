#!/usr/bin/env python3
"""Test a relocated release using only its tools, with deliberate bad overrides.

Requires the generated resync fixtures. This copies the bundle before testing a
missing tool; it never edits the supplied package. No GPU/audio claim is made.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


SRT = "7\n00:00:03,000 --> 00:00:04,000\n<i>世界</i>\n\n9\n00:00:04,100 --> 00:00:05,000\nAfter 100 ms\n\n"


def check(args):
    source = args.bundle.resolve(strict=True)
    output = args.output.resolve()
    if output.exists() or output.is_relative_to(source):
        raise ValueError("Choose a new evidence directory outside the input bundle")
    relative = Path(args.bin)
    if relative.is_absolute() or ".." in relative.parts:
        raise ValueError("--bin must be a path within the bundle")
    output.mkdir(parents=True)
    bundle = output / "Relocated VSF Preview"
    shutil.copytree(source, bundle)
    binaries = bundle / relative
    suffix = ".exe" if os.name == "nt" else ""
    env = dict(os.environ)
    for key in list(env):
        if key.startswith(("DYLD_", "LD_", "PKG_CONFIG_", "VSF_")):
            env.pop(key)
    env["PATH"] = str(Path(env["SystemRoot"]) / "System32") if os.name == "nt" else "/usr/bin:/bin"
    env["VSF_FFMPEG"] = str(output / "must-not-use-development-ffmpeg")
    env["VSF_FFPROBE"] = str(output / "must-not-use-development-ffprobe")
    records = []

    def run(command, expected):
        result = subprocess.run(list(map(str, command)), cwd=output, env=env,
                                capture_output=True, text=True, encoding="utf-8", timeout=150)
        records.append({"command": list(map(str, command)), "exit": result.returncode,
                        "stdout": result.stdout, "stderr": result.stderr})
        if result.returncode != expected:
            raise ValueError(f"Unexpected exit {result.returncode}, expected {expected}: {result.stderr}")
        return result

    evidence = {"scope": "Relocation, bundled analysis and missing-tool failure; no clean-OS/GPU/audio acceptance",
                "status": "incomplete", "commands": records}
    try:
        vsf = binaries / ("vsf" + suffix)
        for name in ["vsf", "ffmpeg", "ffprobe"]:
            run([binaries / (name + suffix), "--version" if name == "vsf" else "-version"], 0)
        original = output / "original.srt"
        original.write_text(SRT, encoding="utf-8")
        fixture = args.fixtures.resolve(strict=True)
        inputs = [original, fixture / "source.mkv", fixture / "offset.mkv", fixture / "speed-ads.mkv"]
        hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}
        for name, expected_timings in [
                ("offset", ["00:00:06,000 --> 00:00:07,000", "00:00:07,100 --> 00:00:08,000"]),
                ("speed-ads", ["00:00:01,960 --> 00:00:02,920", "00:00:03,016 --> 00:00:03,880"])]:
            target = output / (name + ".srt")
            report = output / (name + ".json")
            command = [vsf, "resync", "--source-video", fixture / "source.mkv",
                       "--source-subtitles", original, "--target-video", fixture / (name + ".mkv"),
                       "--output", target, "--report", report, "--json"]
            result = run(command, 0)
            if json.loads(result.stdout)["status"] != "complete":
                raise ValueError("Packaged resync was incomplete")
            actual = target.read_text(encoding="utf-8")
            expected = SRT.replace("00:00:03,000 --> 00:00:04,000", expected_timings[0])
            expected = expected.replace("00:00:04,100 --> 00:00:05,000", expected_timings[1])
            if actual != expected:
                raise ValueError(f"Output text/format/timing mismatch for {name}: {actual!r}")
            saved = target.read_bytes(), report.read_bytes()
            run(command, 2)
            if saved != (target.read_bytes(), report.read_bytes()):
                raise ValueError("Retry changed existing output")
        ffprobe = binaries / ("ffprobe" + suffix)
        ffprobe.rename(ffprobe.with_name(ffprobe.name + ".removed-for-test"))
        result = run([vsf, "resync", "--source-video", fixture / "source.mkv",
                      "--source-subtitles", original, "--target-video", fixture / "offset.mkv",
                      "--output", output / "missing.srt", "--report", output / "missing.json"], 4)
        if "installation is incomplete" not in result.stderr:
            raise ValueError("Missing bundled tool did not report an installation error")
        if (output / "missing.srt").exists() or (output / "missing.json").exists():
            raise ValueError("Missing tool published an output")
        if hashes != {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}:
            raise ValueError("An input changed")
        evidence["status"] = "passed"
    finally:
        (output / "packaged-cli-check.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(f"Passed: {output / 'packaged-cli-check.json'}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--bin", required=True, help="Binary directory relative to bundle root")
    parser.add_argument("--fixtures", type=Path, default=Path("tests/fixtures/generated/resync"))
    parser.add_argument("--output", type=Path, required=True)
    try:
        check(parser.parse_args())
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"Packaged CLI check failed: {error}\n")
