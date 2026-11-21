#!/usr/bin/env python3
"""Verify prerequisites in the unpacked artifact against its ELF receipt and .deb."""
import argparse
import json
from pathlib import Path
import subprocess


def verify(root, deb):
    directory = root / "share/doc/visualsubforge"
    receipt = json.loads((directory / "native-runtime.json").read_text())
    private = {p.name for p in (root / "lib").iterdir()}
    needed = {name for record in receipt["files"] for name in record["needed"]} - private
    rows = [line.split("\t") for line in (directory / "runtime-packages.tsv").read_text().splitlines()]
    if not rows or any(len(row) != 5 or not all(row) for row in rows):
        raise ValueError("Missing runtime package mapping for a target distribution")
    names = [row[0] for row in rows]
    if names != sorted(needed) or set(names) != receipt["system_libraries"].keys():
        raise ValueError("Launcher checks must cover exactly the direct system SONAMEs")
    depends = set()
    for _, older, current, _, _ in rows:
        requirement = "libc6 (>= 2.35)" if current == "libc6" else current
        if current != older:
            requirement += " | " + older
        depends.add(requirement)
    actual = subprocess.check_output(["dpkg-deb", "-f", deb, "Depends"], text=True).strip()
    if actual != ", ".join(sorted(depends)) or sorted(depends) != receipt["platform_dependencies"]:
        raise ValueError(f"Debian Depends disagrees with the launcher providers: {actual}")
    print(f"Verified {len(names)} direct system SONAMEs and {len(depends)} Debian dependencies")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("deb", type=Path)
    args = parser.parse_args()
    verify(args.root, args.deb)
