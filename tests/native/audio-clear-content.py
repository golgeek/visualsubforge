#!/usr/bin/env python3
"""Check gross content continuity across the recorded 1x mpv loop-clear action.

Uses a unique 48 kHz PCM16 reference and process-only stereo float capture.
Requires a matched anchor 100 ms before the command, a phase that cannot wrap
before command completion, and matched continuation through reference EOF.
Exact-silence gaps are reported separately: a content pass is not seamlessness,
sample identity, device presentation, or an accepted editor precision budget.
"""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import wave

import numpy as np

FINGERPRINT = Path(__file__).with_name("audio-fingerprint.py")
spec = importlib.util.spec_from_file_location("fingerprint", FINGERPRINT)
fingerprint = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fingerprint)
PRE_ROLL = 4800


def analyze(reference, captured, start, end, command_start, command_end):
    window = fingerprint.WINDOW
    if (not all(math.isfinite(v) for v in (start, end, command_start, command_end))
            or not 0 <= start < end < len(reference)
            or not PRE_ROLL < command_start <= command_end < len(captured)):
        raise ValueError("invalid source loop or capture command bounds")
    rows = fingerprint.match(reference, captured, fractional=True)
    matched = [r for r in rows if r["kind"] == "matched"]
    anchors = [r for r in matched if r["capture_frame"] + window <= command_start - PRE_ROLL]
    if not anchors:
        return dict(passed=False, failures=["no verified pre-command anchor"])
    anchor = anchors[-1]
    projected_end = anchor["source_frame"] + command_end - anchor["capture_frame"]
    # If the phase could naturally wrap before clear, monotonic correspondence
    # is not an appropriate oracle for this capture. Never select a later anchor
    # from after clear to make a discontinuity disappear.
    if (command_start - PRE_ROLL - anchor["capture_frame"] - window > fingerprint.HOP
            or anchor["source_frame"] < start
            or projected_end > end - 2 * window):
        return dict(passed=False, failures=["pre-command phase does not establish a wrap-free clear"],
                    anchor=anchor, projected_source_at_command_end=projected_end)
    selected = [r for r in rows if r["capture_frame"] >= anchor["capture_frame"]]
    groups = fingerprint.segments(selected)
    gaps = fingerprint.discontinuities(groups, captured)
    matched_count = sum(r["kind"] == "matched" for r in selected)
    failures = []
    if matched_count < fingerprint.MIN_MATCHED_WINDOWS:
        failures.append("insufficient matched coverage")
    if any(g["unexplained_forward_frames"] for g in gaps):
        failures.append("unexplained forward source jump")
    if any(g["unexplained_backward_frames"] for g in gaps):
        failures.append("unexplained repeated source content")
    if any(max(0, g["capture_gap_frames"]) - g["silent_frames"] > 2 * window for g in gaps):
        failures.append("unverified interior content")
    if groups[-1]["source_end"] < len(reference) - 2 * window:
        failures.append("reference EOF not covered")
    # A group can span clear without a mapping discontinuity.
    if not any(g["capture_end"] > command_end + 2 * window and g["source_end"] > end + 2 * window for g in groups):
        failures.append("no verified continuation beyond loop B")
    return dict(passed=not failures, failures=failures, anchor=anchor,
                projected_source_at_command_end=projected_end,
                matched_windows=matched_count, segments=groups, gaps=gaps,
                last_matched_source_frame=groups[-1]["source_end"],
                trailing_unmatched_capture_frames=len(captured) - groups[-1]["capture_end"],
                max_capture_gap_frames=max((g["capture_gap_frames"] for g in gaps), default=0),
                silent_gap_frames=sum(g["silent_frames"] for g in gaps))


def command_bounds(trace, metadata):
    if trace.get("error") or trace.get("capture_exit") != 0 or trace["speed"] != 1 or not trace["looping"]:
        raise ValueError("requires successful 1x looping capture")
    transitions = trace["transitions"]
    if len(transitions) != 1 or transitions[0]["commands"] != [["diagnostic-clear-loop-at-audio-pts"]]:
        raise ValueError("requires exactly one recorded clear action")
    action = transitions[0]
    origin = metadata["blocks"][0]["host_ticks"] * metadata["host_time_numer"] / metadata["host_time_denom"] / 1e9
    before = (trace["sample_origin_monotonic_seconds"] + action["before_seconds"] - origin) * 48000
    after = (trace["sample_origin_monotonic_seconds"] + action["after_seconds"] - origin) * 48000
    bounds = [trace["start"] * 48000, trace["end"] * 48000]
    if not all(math.isfinite(v) and abs(v - round(v)) < 1e-6 for v in bounds):
        raise ValueError("requires sample-aligned loop bounds")
    if not trace["start"] <= action["resolved_seek_seconds"] <= trace["end"]:
        raise ValueError("clear target is outside the loop")
    return [round(v) for v in bounds], [before, after]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("capture", type=Path)
    parser.add_argument("trace", type=Path, help="audio-loop-probe observation JSON")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    with wave.open(str(args.reference), "rb") as source:
        channels = source.getnchannels()
        if channels not in (1, 2) or source.getsampwidth() != 2 or source.getframerate() != 48000:
            raise ValueError("requires mono/stereo 48 kHz PCM16 reference")
        reference = np.frombuffer(source.readframes(source.getnframes()), "<i2").astype(float).reshape(-1, channels) / 32768
    metadata_path = Path(str(args.capture) + ".json")
    metadata = json.loads(metadata_path.read_text())
    captured = np.fromfile(args.capture, "<f4").reshape(-1, 2).astype(float)
    fingerprint.validate_metadata(metadata, len(captured))
    loop, command = command_bounds(json.loads(args.trace.read_text()), metadata)
    results = [analyze(reference[:, min(ch, channels - 1)], captured[:, ch], *loop, *command) for ch in range(2)]
    report = dict(passed=all(r["passed"] for r in results), channels=results,
                  loop_source_frames=loop, command_capture_frames=command,
                  scope=__doc__, pre_roll_frames=PRE_ROLL, window_frames=fingerprint.WINDOW,
                  transition_allowance_frames=2 * fingerprint.WINDOW,
                  input_sha256={str(p.resolve()): hashlib.sha256(p.read_bytes()).hexdigest()
                                for p in (args.reference, args.capture, metadata_path, args.trace, Path(__file__), FINGERPRINT)})
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(dict(passed=report["passed"], report=str(args.output))))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
