#!/usr/bin/env python3
"""Validate libass-revision-probe JSONL and summarize measured CPU durations."""

import argparse
import json
import math
from pathlib import Path
import statistics


def require(condition, message):
    if not condition:
        raise ValueError(message)


def analyze(path):
    records = [json.loads(line) for line in path.read_text().splitlines()]
    require(len(records) == 129, f"{path}: expected header and 128 revisions")
    header, rows = records[0], records[1:]
    require(header["iterations_per_mode"] == 32, "unexpected revision count")
    require(type(header["reuse_first"]) is bool, "missing execution order")
    fonts = {"macos": ["Hiragino Sans", "HiraginoSans-W4"],
             "noto": ["Noto Sans", "NotoSans-Regular"]}[header["font_preset"]]
    order = [header["reuse_first"], not header["reuse_first"]]
    expected = [(font, reuse, revision)
                for font in fonts for reuse in order for revision in range(32)]
    require([(r["font"], r["reuse"], r["revision"]) for r in rows] == expected,
            "missing, duplicate or reordered records")
    for row in rows:
        require(row["width"] == (1280 if row["revision"] // 8 % 2 else 640),
                "unexpected frame size")
        require(row["boundary_checks"] is True and row["images"] > 0,
                "missing bitmap/boundary validation")
        for field in ["setup_ms", "parse_ms", "render_ms", "total_ms"]:
            require(math.isfinite(row[field]) and row[field] >= 0, "invalid duration")
        require(abs(sum(row[f] for f in ["setup_ms", "parse_ms", "render_ms"])
                    - row["total_ms"]) <= 0.000004, "timing components disagree")
    groups = []
    for font in fonts:
        modes = {reuse: [r for r in rows if r["font"] == font and r["reuse"] == reuse]
                 for reuse in order}
        for fresh, reused in zip(modes[False], modes[True]):
            require((fresh["hash"], fresh["images"]) == (reused["hash"], reused["images"]),
                    "fresh and reused renderers differ")
        for reuse in order:
            values = modes[reuse]
            require(len({r["hash"] for r in values}) == 32, "revision did not change output")
            warm = sorted(r["total_ms"] for r in values[1:])
            groups.append({"font": font, "reuse": reuse,
                           "first_total_ms": values[0]["total_ms"],
                           "warm_median_ms": statistics.median(warm),
                           "warm_p95_ms": warm[math.ceil(len(warm) * .95) - 1],
                           "warm_max_ms": max(warm)})
    return {"file": str(path), "header": header, "revisions": 128,
            "cross_mode_bitmap_matches": 64, "groups": groups}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", type=Path, nargs="+")
    args = parser.parse_args()
    try:
        results = [analyze(path) for path in args.logs]
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"FAIL: {error}\n")
    print(json.dumps({"status": "PASS", "runs": results}, indent=2))


if __name__ == "__main__":
    main()
