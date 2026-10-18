#!/usr/bin/env python3
"""Render production ASS/SSA packet sources with mpv/libass, without a desktop.

First run cargo test --locked -p visualsubforge native_demux -- --ignored.
This checks software composition, not desktop/GPU/audio/platform acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def run(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, timeout=30, **kwargs)


def pixels(rgb, predicate, start, end):
    return sum(predicate(*rgb[i:i + 3]) for i in range(start * 768 * 3, end * 768 * 3, 3))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    args.output.mkdir(parents=True, exist_ok=False)
    receipt = {
        "scope": "Software composition only; not desktop/GPU/audio/platform acceptance",
        "mpv": str(args.mpv.resolve()),
        "mpv_sha256": hashlib.sha256(args.mpv.read_bytes()).hexdigest(),
        "version": run([str(args.mpv), "--version"], text=True).stdout,
        "sources": {}, "frames": {},
    }
    for extension in ("ass", "ssa"):
        source = root / f"tests/fixtures/generated/ass-ssa/native/{extension}.url"
        url = source.read_text()
        assert url.startswith("hex://"), "Run the production serializer test first"
        receipt["sources"][extension] = hashlib.sha256(source.read_bytes()).hexdigest()
        for name, seconds in (("visible", 1.5), ("expired", 4.1)):
            destination = args.output / f"{extension}-{name}"
            destination.mkdir()
            output = run([
                str(args.mpv), "--no-config", "--load-scripts=no", "--osc=no", "--ytdl=no",
                "--vo=image", "--vo-image-format=png", f"--vo-image-outdir={destination}",
                "--ao=null", "--sub-ass-override=no", f"--sub-file={url}",
                f"--start={seconds}", "--frames=1", "av://lavfi:color=c=black:s=768x576:d=6",
            ], text=True)
            (destination / "mpv.log").write_text((output.stdout + output.stderr).replace(url, "<VSF source>"))
            png = destination / "00000001.png"
            rgb = run(["ffmpeg", "-v", "error", "-i", str(png), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout
            assert len(rgb) == 768 * 576 * 3
            frame = {
                "red_top": pixels(rgb, lambda r, g, b: r > 120 and r > 2 * g and r > 2 * b, 0, 160),
                "white_bottom": pixels(rgb, lambda r, g, b: min(r, g, b) > 180, 300, 576),
                "nonblack": pixels(rgb, lambda r, g, b: max(r, g, b) > 30, 0, 576),
            }
            receipt["frames"][f"{extension}-{name}"] = frame
            if name == "visible":
                assert frame["red_top"] > 100, f"{extension}: Title style color/alignment missing"
                assert frame["white_bottom"] > 100, f"{extension}: Default style/text missing"
            else:
                assert frame["nonblack"] == 0, f"{extension}: expired text remains"
    receipt["passed"] = True
    (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("ASS/SSA document software-composition passed:", args.output / "receipt.json")


if __name__ == "__main__":
    main()
