#!/usr/bin/env python3
"""Prepare, build and test one pinned mpv variant in a new directory.

Never installs libraries. A receipt records observed dependencies and binaries;
passing upstream tests does not establish editor, GPU or audio acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from native_evidence import digest


ROOT = Path(__file__).resolve().parent.parent


def capture(args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True, stderr=subprocess.STDOUT).strip()


def source_identity(source):
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=source
    ).split(b"\0")
    files = {os.fsdecode(path): digest(source / os.fsdecode(path))
             for path in sorted(filter(None, paths))}
    return {"commit": capture(["git", "rev-parse", "HEAD"], source),
            "files_sha256": files}


def validate_results(declared, results, variant):
    expected = {test["name"] for test in declared}
    # Meson prefixes names with the project and, when present, the suite.
    actual = {test["name"].rsplit(":", 1)[-1]: test["result"] for test in results}
    required = {"libmpv-lifetime"}
    if variant != "render":
        required |= {"queued-pts", "aframe-clip"}
    if (len(results) != len(actual) or not required <= expected
            or expected != actual.keys()):
        raise ValueError("Missing declared/required tests or duplicate results; inspect Meson logs")
    if any(result not in {"OK", "SKIP"} for result in actual.values()):
        raise ValueError("Upstream tests did not all pass or explicitly skip")
    if any(actual[name] != "OK" for name in required):
        raise ValueError("Required lifetime/audio policy tests must execute and pass")
    return {result: list(actual.values()).count(result) for result in ["OK", "SKIP"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True, help="Existing upstream Git clone")
    parser.add_argument("--output", type=Path, required=True, help="New evidence directory")
    parser.add_argument("--variant", choices=["render", "baseline", "candidates"], required=True)
    parser.add_argument("--jobs", type=int, default=6)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 64:
        parser.error("--jobs must be between 1 and 64")
    source = args.source.resolve(strict=True)
    tools = {}
    for name in ["git", "meson", "ninja"]:
        executable = shutil.which(name)
        if not executable:
            parser.error(f"Missing {name}; see docs/native-runtime.md")
        tools[name] = {"path": executable, "version": capture([executable, "--version"])}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    receipt = {
        "schema": 1, "status": "incomplete", "variant": args.variant,
        "started_utc": datetime.now(timezone.utc).isoformat(),
        "host": {"system": platform.system(), "release": platform.release(),
                 "machine": platform.machine(), "python": platform.python_version()},
        "tools": tools, "commands": [],
        "script_sha256": digest(Path(__file__)),
        "support_scripts": {name: digest(ROOT / "scripts" / name)
                            for name in ["prepare-mpv-clock.py", "native_evidence.py"]},
        "scope": "Source build and upstream tests; no device/editor acceptance or installation",
    }
    build = output / "build"
    prepared = output / "prepared"
    try:
        with (output / "commands.log").open("w") as log:
            def run(command):
                command = list(map(str, command))
                receipt["commands"].append(command)
                print("RUN", json.dumps(command), flush=True)
                log.write("RUN " + json.dumps(command) + "\n")
                log.flush()
                subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)

            prepare = [sys.executable, ROOT / "scripts/prepare-mpv-clock.py",
                       "--source", source, "--output", prepared]
            if args.variant == "render":
                prepare.append("--render-only")
            elif args.variant == "candidates":
                prepare.append("--audio-candidates")
            run(prepare)
            checkout = prepared / "source"
            receipt["recipe"] = json.loads((prepared / "manifest.json").read_text())
            receipt["source"] = source_identity(checkout)
            options = ["-Dlibmpv=true", "-Dtests=true", "-Dmanpage-build=disabled",
                       "-Dvapoursynth=disabled"]
            if platform.system() == "Darwin":
                options += ["-Dcocoa=enabled", "-Dswift-build=enabled", "-Dmacos-media-player=disabled"]
            run(["meson", "setup", build, checkout, *options])
            run(["meson", "compile", "-C", build, "-j", args.jobs,
                 "mpv:executable", "mpv:shared_library"])
            run(["meson", "test", "-C", build, "--print-errorlogs", "-j", args.jobs])
        info = build / "meson-info"
        for name in ["compilers", "machines", "dependencies", "buildoptions", "tests"]:
            receipt[name] = json.loads((info / f"intro-{name}.json").read_text())
        results = [json.loads(line) for line in (build / "meson-logs/testlog.json").read_text().splitlines()]
        # Meson's raw log can include the entire inherited environment. Never
        # copy it into the shareable receipt or CI artifacts.
        receipt["test_results"] = [
            {key: test[key] for key in ["name", "result", "duration", "returncode", "command"]}
            for test in results
        ]
        receipt["counts"] = validate_results(receipt["tests"], results, args.variant)
        if source_identity(checkout) != receipt["source"]:
            raise ValueError("Prepared source changed during the build")
        targets = json.loads((info / "intro-targets.json").read_text())
        receipt["binaries"] = [
            {"path": path, "sha256": digest(Path(path)), "type": target["type"]}
            for target in targets if target["name"] == "mpv"
            for path in target["filename"]
        ]
        if not any(binary["type"] == "shared library" for binary in receipt["binaries"]):
            raise ValueError("The selected build did not produce a libmpv shared library")
        receipt["status"] = "passed"
        print(f"PASS: {receipt['counts']} — {output / 'build-receipt.json'}")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        receipt["status"] = "failed"
        receipt["error"] = str(error)
        raise
    finally:
        receipt["finished_utc"] = datetime.now(timezone.utc).isoformat()
        (output / "build-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Native build check failed: {error}") from error
