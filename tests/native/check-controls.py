#!/usr/bin/env python3
"""Run every Python oracle's synthetic controls; never capture or play audio."""
from pathlib import Path
import subprocess
import sys


def main():
    directory = Path(__file__).resolve().parent
    tests = sorted(directory.glob("*-test.py"))
    if not tests:
        raise SystemExit("No native oracle control suites found")
    for test in tests:
        print(f"Running {test.name}", flush=True)
        subprocess.run([sys.executable, str(test)], check=True, timeout=300)
    print(f"PASS: {len(tests)} synthetic oracle suites; no native runtime claim")


if __name__ == "__main__":
    main()
