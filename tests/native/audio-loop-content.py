#!/usr/bin/env python3
"""Conservative content oracle for steady 1x loops in a unique PCM fixture.

Capture interval must exclude startup, pause, seek and loop-clear transitions.
Range and interval use integer 48 kHz frames. This checks captured content,
not device presentation, sample-perfect boundaries or an editor error budget.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import wave

import numpy as np

FINGERPRINT = Path(__file__).with_name("audio-fingerprint.py")
spec = importlib.util.spec_from_file_location("fingerprint", FINGERPRINT)
fingerprint = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fingerprint)


def analyze(reference, captured, start, end, *, fractional=False):
    window = fingerprint.WINDOW
    if not 0 <= start < end <= len(reference) or end - start < 8 * window:
        raise ValueError("loop must be within reference and at least eight windows long")
    rows = fingerprint.match(reference, captured, fractional=fractional)
    groups = fingerprint.segments(rows)
    matched = [row for row in rows if row["kind"] == "matched"]
    outside = [row for row in matched if row["source_frame"] < start - window
               or row["source_frame"] + window > end + window]
    # Only recognize an observed B-to-A transition near both boundaries.
    # A modulo operation on arbitrary backward jumps would conceal defects.
    unfolded = []
    wraps = []
    uncertain_boundaries = []
    offset = 0
    for index, group in enumerate(groups):
        if index:
            before = groups[index - 1]
            if (abs(before["source_end"] - end) <= 2 * window
                    and abs(group["source_start"] - start) <= 2 * window
                    and before["source_end"] - group["source_start"] > (end - start) / 2):
                offset += end - start
                wraps.append(group["capture_start"])
            elif (group["source_start"] < before["source_end"]
                  and group["capture_start"] - before["capture_end"] > 2 * window
                  and group["source_start"] - before["source_end"] + end - start
                  <= group["capture_start"] - before["capture_end"] + 2 * window):
                # Missing matches may straddle a valid wrap. Do not label that
                # uncertainty as proven replay, nor count it as a verified wrap.
                uncertain_boundaries.append(index - 1)
                offset += end - start
        unfolded.append({**group, "source_start": group["source_start"] + offset,
                         "source_end": group["source_end"] + offset})
    gaps = fingerprint.discontinuities(unfolded, captured)
    for index in uncertain_boundaries:
        gaps[index]["unverified_boundary"] = True
    verified_gaps = [g for g in gaps if not g.get("unverified_boundary")]
    failures = []
    if len(matched) < fingerprint.MIN_MATCHED_WINDOWS:
        failures.append("insufficient matched coverage")
    if len(wraps) < 2:
        failures.append("fewer than two observed boundary wraps")
    if outside:
        failures.append("matched content outside loop range")
    if any(g["unexplained_forward_frames"] for g in verified_gaps):
        failures.append("unexplained forward source jump")
    if any(g["unexplained_backward_frames"] for g in verified_gaps):
        failures.append("unexplained repeated source content")
    if any(g["capture_gap_frames"] > 2 * window for g in gaps):
        failures.append("unverified interior capture interval")
    if uncertain_boundaries:
        failures.append("unverified loop boundary")
    return dict(passed=not failures, failures=failures,
                loop_source_frames=[start, end], matched_windows=len(matched),
                outside_range_windows=len(outside), wrap_capture_frames=wraps,
                unverified_boundary_transitions=uncertain_boundaries,
                unmatched_leading_frames=groups[0]["capture_start"] if groups else len(captured),
                unmatched_trailing_frames=len(captured) - groups[-1]["capture_end"] if groups else len(captured),
                segments=groups, gaps=gaps)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--loop", type=int, nargs=2, required=True, metavar=("A", "B"))
    parser.add_argument("--interval", type=int, nargs=2, required=True, metavar=("START", "END"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--fractional", action="store_true",
                        help="fit a bounded adjacent-sample interpolation per window")
    args = parser.parse_args()
    with wave.open(str(args.reference), "rb") as source:
        channels = source.getnchannels()
        if channels not in (1, 2) or source.getsampwidth() != 2 or source.getframerate() != 48000:
            raise ValueError("requires mono/stereo 48 kHz PCM16 reference")
        reference = np.frombuffer(source.readframes(source.getnframes()), "<i2")
        reference = reference.astype(float).reshape(-1, channels) / 32768
    metadata = Path(str(args.capture) + ".json")
    captured = np.fromfile(args.capture, "<f4").reshape(-1, 2).astype(float)
    fingerprint.validate_metadata(json.loads(metadata.read_text()), len(captured))
    first, last = args.interval
    if not 0 <= first < last <= len(captured):
        raise ValueError("interval outside capture")
    results = [analyze(reference[:, min(ch, channels - 1)], captured[first:last, ch],
                       *args.loop, fractional=args.fractional) for ch in range(2)]
    report = dict(passed=all(r["passed"] for r in results), channels=results,
                  capture_interval_frames=args.interval,
                  fractional_interpolation=args.fractional,
                  scope=__doc__, window_frames=fingerprint.WINDOW,
                  hop_frames=fingerprint.HOP, edge_allowance_frames=fingerprint.WINDOW,
                  transition_allowance_frames=2 * fingerprint.WINDOW,
                  input_sha256={str(p.resolve()): hashlib.sha256(p.read_bytes()).hexdigest()
                                for p in (args.reference, args.capture, metadata, Path(__file__), FINGERPRINT)})
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(dict(passed=report["passed"], report=str(args.output))))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
