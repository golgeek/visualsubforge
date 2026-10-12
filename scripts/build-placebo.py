#!/usr/bin/env python3
"""Build the pinned shared libplacebo outside its source tree; retain its sources."""
import argparse
import json
from pathlib import Path
import subprocess
from native_evidence import digest
from source_archive import archive_source


COMMIT = "cee9b076f2c63104ccfd497fa79c39a867293ec4"  # v7.360.1
URL = "https://code.videolan.org/videolan/libplacebo.git"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=3)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 64:
        parser.error("--jobs must be between 1 and 64")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    source, build, sdk = root / "source", root / "build", root / "sdk"
    commands = []
    receipt = {"status": "incomplete", "url": URL, "commit": COMMIT, "commands": commands}
    try:
        with (root / "commands.log").open("w") as log:
            def run(*command):
                command = list(map(str, command))
                commands.append(command)
                print("RUN", json.dumps(command), flush=True)
                log.write(json.dumps(command) + "\n")
                log.flush()
                subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)

            run("git", "clone", "--no-checkout", URL, source)
            run("git", "-C", source, "checkout", "--detach", COMMIT)
            run("git", "-C", source, "submodule", "update", "--init", "--recursive")
            receipt["submodules"] = subprocess.check_output(
                ["git", "-C", str(source), "submodule", "status", "--recursive"], text=True)
            archive_source(source, root / "source.tar.gz")
            receipt["source_sha256"] = digest(root / "source.tar.gz")
            run("meson", "setup", build, source, "--buildtype=release", "--wrap-mode=nodownload",
                f"--prefix={sdk}", "--libdir=lib", "-Ddefault_library=shared",
                "-Ddemos=false", "-Dtests=true", "-Dopengl=enabled")
            run("meson", "compile", "-C", build, "-j", args.jobs)
            # Upstream's GPU tests may skip without a device; retain that fact.
            run("meson", "test", "-C", build, "--print-errorlogs", "-j", args.jobs)
            tests = [json.loads(line) for line in (build / "meson-logs/testlog.json").read_text().splitlines()]
            receipt["tests"] = [{key: test[key] for key in ("name", "result", "returncode")}
                                for test in tests]
            run("meson", "install", "-C", build)
            receipt["status"] = "passed"
    finally:
        (root / "build-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
