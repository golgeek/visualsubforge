#!/usr/bin/env python3
"""Experimental H.264 candidate study. Does not change VSF or its saved projects."""
import argparse
import concurrent.futures
import csv
import io
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import time


def compare(candidates, reference, tolerance=0.001):
    """One-to-one matching; agreement with a detector is NOT ground-truth accuracy."""
    left, right = sorted(candidates), sorted(reference)
    i = j = matched = 0
    while i < len(left) and j < len(right):
        if abs(left[i] - right[j]) <= tolerance:
            matched += 1
            i += 1
            j += 1
        elif left[i] < right[j]:
            i += 1
        else:
            j += 1
    return {"candidates": len(left), "reference": len(right), "matched": matched,
            "reference_coverage": matched / len(right) if right else None}


def extract(binary, media):
    result = subprocess.run([str(binary.resolve()), str(media)], capture_output=True,
                            text=True, check=True, timeout=60)
    rows = [{"pts": float(r["pts"]), "size": int(r["size"]),
             "key": r["key"] == "1", "type": r["type"],
             "reference": r["reference"] == "1"}
            for r in csv.DictReader(io.StringIO(result.stdout))]
    rows.sort(key=lambda r: r["pts"])
    if any(a["pts"] >= b["pts"] for a, b in zip(rows, rows[1:])):
        raise ValueError("Experiment requires unique presentation timestamps")
    return rows, result.stdout


def verify(media, pts, stream_index):
    # Small neighborhood supplies preceding images. FFmpeg performs accurate seek
    # and decodes prerequisite frames from the earlier random-access point.
    # The experiment is CFR25-specific here; candidate extraction itself uses PTS.
    command = [os.environ.get("VSF_FFMPEG", "ffmpeg"), "-v", "error", "-nostdin",
               "-threads", "2", "-ss", f"{max(0, pts - .12):.9f}", "-t", "0.28",
               "-copyts", "-i", str(media), "-map", f"0:{stream_index}",
               "-an", "-sn", "-dn", "-filter_threads", "1", "-vf",
               "scale=320:-2,settb=expr=1/1000000,scdet=threshold=1,"
               "metadata=mode=print:key=lavfi.scd.score:file=-",
               "-fps_mode", "passthrough", "-f", "null", "-"]
    result = subprocess.run(command, capture_output=True, text=True, check=True, timeout=30)
    frame_pts = None
    scores = []
    for line in result.stdout.splitlines():
        if line.startswith("frame:"):
            frame_pts = int(line.split("pts:", 1)[1].split()[0]) / 1_000_000
        elif line.startswith("lavfi.scd.score="):
            scores.append((frame_pts, float(line.split("=", 1)[1])))
    own = [score for t, score in scores if abs(t - pts) < .001]
    if len(own) != 1 or len([t for t, _ in scores if t < pts]) < 2:
        raise ValueError(f"Candidate {pts} lacks a unique image and preceding context")
    return {"seconds": pts, "score": own[0], "neighbors": scores}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("media", type=Path)
    parser.add_argument("--extractor", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True,
                        help="scene-benchmark.py report containing detect-640-28")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--workers", type=int, default=8)
    args = parser.parse_args()
    if not 1 <= args.workers <= 8:
        parser.error("workers must be between 1 and 8")
    baseline = json.loads(args.baseline.read_text())
    origin = float(baseline["media"]["format"]["start_time"])
    reference = next(r["cuts"] for r in baseline["runs"] if r["case"] == "detect-640-28")
    # The baseline is user-supplied evidence for the SAME input. It is not a cache
    # identity check. The file size/metadata below make mismatched reports auditable.
    probe_start = time.perf_counter()
    probe = json.loads(subprocess.check_output([
        os.environ.get("VSF_FFPROBE", "ffprobe"), "-v", "error", "-select_streams", "V:0",
        "-show_entries", "stream=index,codec_name,width,height,avg_frame_rate:format=duration,start_time",
        "-of", "json", str(args.media)], timeout=30))
    probe_seconds = time.perf_counter() - probe_start
    if probe != baseline["media"]:
        raise ValueError("Baseline media metadata differs; supply a matching baseline")
    args.output.mkdir(parents=True, exist_ok=True)
    timings = []
    for _ in range(3):
        started = time.perf_counter()
        rows, raw = extract(args.extractor, args.media)
        candidates = [r["pts"] - origin for r in rows if r["type"] == "I" and not r["key"]]
        timings.append(time.perf_counter() - started)
    (args.output / "packets.csv").write_text(raw)
    report = {"media": probe, "bytes": args.media.stat().st_size,
              "probe_seconds": probe_seconds, "extraction_seconds": timings,
              "ffmpeg": subprocess.check_output([os.environ.get("VSF_FFMPEG", "ffmpeg"), "-version"], text=True).splitlines()[0],
              "cache": "Uncontrolled OS cache, no cold-cache claim",
              "nonkey_I": compare(candidates, reference),
              "keyframes": compare([r["pts"] - origin for r in rows if r["key"]], reference),
              "all_I": compare([r["pts"] - origin for r in rows if r["type"] == "I"], reference)}
    groups = {}
    for row in rows:
        groups.setdefault((row["type"], row["reference"]), []).append(row)
    ranked = []
    for group in groups.values():
        for i, row in enumerate(group):
            neighbors = [r["size"] for r in group[max(0, i-12):i] + group[i+1:i+13]]
            if neighbors:
                ranked.append((row["size"] / max(1, statistics.median(neighbors)), row["pts"] - origin))
    report["size_spikes"] = {str(th): compare([t for ratio, t in ranked if ratio >= th], reference)
                             for th in (1.5, 2, 3, 4, 6)}
    matched, extra = [], []
    for i, row in enumerate(rows):
        if row["type"] == "I" and not row["key"]:
            t = row["pts"] - origin
            is_match = any(abs(t-x) < .001 for x in reference)
            (matched if is_match else extra).append({"frame": i, "seconds": t})
    rng = random.Random(8003)
    sample = sorted([dict(r, stratum="standard-match") for r in rng.sample(matched, min(20, len(matched)))] +
                    [dict(r, stratum="standard-extra") for r in rng.sample(extra, min(40, len(extra)))],
                    key=lambda r: r["seconds"])
    (args.output / "review-sample.json").write_text(json.dumps(sample, indent=2) + "\n")
    if args.verify:
        if origin != 0 or probe["streams"][0]["avg_frame_rate"] != "25/1":
            raise ValueError("Local verification experiment is limited to zero-origin CFR25")
        started = time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
            scores = list(pool.map(lambda t: verify(args.media, t, probe["streams"][0]["index"]), candidates))
        elapsed = time.perf_counter() - started
        report["verification_seconds"] = elapsed
        report["workers"] = args.workers
        report["verified_thresholds"] = {str(th): compare([r["seconds"] for r in scores if r["score"] >= th], reference)
                                         for th in (1, 2, 3, 4, 5, 6, 8, 10)}
        (args.output / "verification.json").write_text(json.dumps({"seconds": elapsed, "scores": scores}, indent=2) + "\n")
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k:v for k,v in report.items() if k not in ("media", "ffmpeg", "size_spikes")}, indent=2))


if __name__ == "__main__":
    main()
