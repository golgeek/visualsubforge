#!/usr/bin/env python3
"""Stage the Linux probe using the six-second composition MP4/SRT fixture.

Requires host FFmpeg. No downloads or VM changes; see task 003c for guest build.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
from composition_fixture import decoded_peaks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--media", type=Path, required=True)
    parser.add_argument("--subtitles", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    peaks, count = decoded_peaks(args.media)
    if count != 288000:
        parser.error("This bounded probe requires the six-second composition fixture")
    root = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    shutil.copy(args.media, output / "composition.mp4")
    shutil.copy(args.subtitles, output / "preview.srt")
    shutil.copy(root / "tests/native/linux-composition-probe.c", output)
    page = (root / "tests/native/linux-composition.html").read_text()
    (output / "composition.html").write_text(page.replace("__VSF_PEAKS__", json.dumps(peaks)))
    (output / "manifest.json").write_text(json.dumps({
        "decoded_samples": count, "peak_bins": len(peaks),
        "files": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in sorted(output.iterdir()) if p.is_file()},
    }, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
