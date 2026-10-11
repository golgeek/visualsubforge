#!/usr/bin/env python3
"""Compare scene pipelines on the same local video without writing decoded frames."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("media", type=Path)
parser.add_argument("--seconds", type=float, default=60, help="0 scans the entire file")
parser.add_argument("--repeats", type=int, default=3)
parser.add_argument("--cases", nargs="+", help="Run only named cases")
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
if args.seconds < 0 or args.repeats < 1:
    parser.error("seconds must be nonnegative and repeats must be positive")
ffmpeg = os.environ.get("VSF_FFMPEG", "ffmpeg")
ffprobe = os.environ.get("VSF_FFPROBE", "ffprobe")
probe = json.loads(subprocess.check_output([
    ffprobe, "-v", "error", "-select_streams", "V:0", "-show_entries",
    "stream=index,codec_name,width,height,avg_frame_rate:format=duration,start_time",
    "-of", "json", str(args.media),
]))
stream = probe["streams"][0]
origin = float(probe["format"]["start_time"])
old_scale = "scale=w='min(640,iw)':h=-2"
small_scale = "scale=w=-2:h='min(64,ih)':flags=bilinear"
detection = ",settb=expr=1/1000000,scdet=threshold=10:sc_pass=1,metadata=mode=print:key=lavfi.scd.time:file=-"
cases = [
    ("decode-2", 2, None),
    ("scale-640-2", 2, old_scale),
    ("detect-640-2", 2, old_scale + detection),
    ("scale-64-2", 2, small_scale),
    ("detect-64-2", 2, small_scale + detection),
    ("decode-8", 8, None),
    ("detect-64-8", 8, small_scale + detection),
    ("detect-640-8", 8, old_scale + detection),
]
# Additional named decoder sweep cases include deliberate oversubscription.
if args.cases:
    for threads in (0, 1, 4, 12, 14, 16, 24, 28, 32, 48, 64, 96):
        cases.append((f"detect-64-{threads}", threads, small_scale + detection))
        cases.append((f"detect-640-{threads}", threads, old_scale + detection))
    unknown = set(args.cases) - {case[0] for case in cases}
    if unknown:
        parser.error(f"Unknown cases: {sorted(unknown)}")
    cases = [case for case in cases if case[0] in args.cases]
report = {"media": probe, "input_seconds": args.seconds, "repeats": args.repeats,
          "ffmpeg": subprocess.check_output([ffmpeg, "-version"], text=True).splitlines()[0],
          "cache": "Uncontrolled OS cache; no cold-cache claim", "runs": []}
for repeat in range(args.repeats):
    # Reverse order on alternate passes to reduce a fixed warm-cache/order bias.
    for name, threads, filters in cases[::1 if repeat % 2 == 0 else -1]:
        command = [ffmpeg, "-v", "error", "-nostdin", "-threads", str(threads), "-copyts"]
        if args.seconds:
            command += ["-t", str(args.seconds)]  # Input bound: sc_pass may emit few frames.
        command += ["-i", str(args.media), "-map", f"0:{stream['index']}",
                    "-an", "-sn", "-dn", "-filter_threads", "1"]
        if filters:
            command += ["-vf", filters]
        command += ["-fps_mode", "passthrough", "-f", "null", "-"]
        started = time.perf_counter()
        result = subprocess.run(command, capture_output=True, text=True, timeout=1800, check=True)
        elapsed = time.perf_counter() - started
        cuts = [int(line.split("pts:", 1)[1].split()[0]) / 1_000_000 - origin
                for line in result.stdout.splitlines() if line.startswith("frame:")]
        cuts = sorted(set(t for t in cuts if t > 0))
        report["runs"].append({"case": name, "repeat": repeat + 1,
                               "seconds": elapsed, "cuts": cuts})
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        print(f"{name}: {elapsed:.3f}s, {len(cuts)} cuts", flush=True)
for name, _, _ in cases:
    elapsed = [r["seconds"] for r in report["runs"] if r["case"] == name]
    print(f"median {name}: {statistics.median(elapsed):.3f}s")
