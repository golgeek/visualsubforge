#!/usr/bin/env python3
"""Validate the Mac editor re-entry/replacement lane's process-only PCM.

Uses the existing conservative fingerprint/loop oracles. This does not establish
sample identity, physical presentation, startup latency or a product error budget.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import wave

import numpy as np


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


loop = load("editor_loop", "audio-loop-content.py")
fingerprint = loop.fingerprint


def suffix(reference, captured, start):
    if not 0 <= start < len(reference):
        raise ValueError("suffix start outside reference")
    rows = fingerprint.match(reference, captured, fractional=True)
    matched = [r for r in rows if r["kind"] == "matched"]
    groups = fingerprint.segments(rows)
    gaps = fingerprint.discontinuities(groups, captured)
    failures = []
    window = fingerprint.WINDOW
    if len(matched) < fingerprint.MIN_MATCHED_WINDOWS:
        failures.append("insufficient matched coverage")
    if not groups or abs(groups[0]["source_start"] - start) > 2 * window:
        failures.append("requested suffix beginning not covered")
    if not groups or groups[-1]["source_end"] < len(reference) - 2 * window:
        failures.append("reference EOF not covered")
    if any(g["unexplained_forward_frames"] for g in gaps):
        failures.append("unexplained forward source jump")
    if any(g["unexplained_backward_frames"] for g in gaps):
        failures.append("unexplained repeated source content")
    if any(max(0, g["capture_gap_frames"]) - g["silent_frames"] > 2 * window for g in gaps):
        failures.append("unverified interior content")
    # These suffixes play continuously, with no intentional held pause. The
    # generated reference is non-silent; a full silent window inside it is a
    # failure even when source order resumes correctly afterward.
    if any(g["silent_frames"] for g in gaps):
        failures.append("unexpected interior silence")
    return dict(passed=not failures, failures=failures, matched_windows=len(matched),
                segments=groups, gaps=gaps, start_source_frame=start,
                reference_frames=len(reference))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    if report["status"] != "state-and-capture-complete-analysis-required":
        raise ValueError("editor state/capture workflow is incomplete")
    expected = {"audio-loop-initial", "audio-past-b", "audio-loop-reentered",
                "audio-loop-after-lead-in", "audio-replacement"}
    if len(report["captures"]) != len(expected) or {r["name"] for r in report["captures"]} != expected:
        raise ValueError("incomplete or duplicate capture matrix")
    for name, sha256 in report["sources"].items():
        if digest(Path(name)) != sha256:
            raise ValueError("changed source: " + name)
    results = []
    for row in report["captures"]:
        path = Path(row["path"])
        metadata_path = Path(str(path) + ".json")
        if (digest(path) != row["sha256"] or digest(metadata_path) != row["metadataSha256"]
                or row["exit"] != {"code": 0, "signal": None}):
            raise ValueError("changed or failed capture: " + row["name"])
        reference_path = Path(row["reference"])
        if str(reference_path) not in report["sources"]:
            raise ValueError("unverified reference source")
        with wave.open(str(reference_path)) as source:
            if (source.getframerate(), source.getnchannels(), source.getsampwidth()) != (48000, 2, 2):
                raise ValueError("requires stereo 48 kHz PCM16 reference")
            reference = np.frombuffer(source.readframes(source.getnframes()), "<i2")
            reference = reference.reshape(-1, 2).astype(float) / 32768
        captured = np.fromfile(path, "<f4").reshape(-1, 2).astype(float)
        metadata = json.loads(metadata_path.read_text())
        fingerprint.validate_metadata(metadata, len(captured))
        if metadata["pid"] != report["pid"]:
            raise ValueError("capture does not belong to the owned editor process")
        if row["kind"] == "loop":
            a, b = row["loop"]
            channels = [loop.analyze(reference[:, ch], captured[:, ch], round(a * 48000),
                                     round(b * 48000), fractional=True) for ch in range(2)]
        elif row["kind"] == "suffix":
            channels = [suffix(reference[:, ch], captured[:, ch], round(row["start"] * 48000))
                        for ch in range(2)]
        else:
            raise ValueError("unknown capture kind")
        result = dict(name=row["name"], passed=all(c["passed"] for c in channels), channels=channels)
        results.append(result)
        print(row["name"], result["passed"], [c["failures"] for c in channels], flush=True)
    output = dict(passed=all(r["passed"] for r in results), scope=__doc__, cases=results,
                  input_sha256={str(p.resolve()): digest(p) for p in (
                      args.report, Path(__file__), Path(loop.__file__), Path(fingerprint.__file__))})
    with args.output.open("x") as stream:
        json.dump(output, stream, indent=2)
        stream.write("\n")
    return 0 if output["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
