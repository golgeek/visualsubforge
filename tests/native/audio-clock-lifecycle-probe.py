#!/usr/bin/env python3
"""Observe queued clock behavior across rate, output and file replacement.

This is a diagnostic: process output is captured before the DAC, and summary
ranges do not establish sample-accurate audible alignment or a passing gate.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def summarize(trace, capture_reader):
    if "error" in trace:
        return {"case": trace["case"], "error": trace["error"]}
    phases = {}
    for sample in trace["samples"]:
        phase = phases.setdefault(sample["phase"], {"samples": 0, "positions": [], "unavailable": 0,
            "outputs": set(), "paths": set(), "speeds": set()})
        phase["samples"] += 1
        for prop, key in [("current-ao", "outputs"), ("path", "paths"), ("speed", "speeds")]:
            if sample[prop] is not None:
                phase[key].add(sample[prop])
        if sample["audio-pts"] is None:
            phase["unavailable"] += 1
        elif not sample["seeking"]:
            phase["positions"].append(sample["audio-pts"])
    for phase in phases.values():
        positions = phase.pop("positions")
        phase["raw_audio_pts_range"] = [min(positions), max(positions)] if positions else None
        for key in ["outputs", "paths", "speeds"]:
            phase[key] = sorted(phase[key])
    windows, window_seconds = capture_reader(trace)
    transitions = [(0, "initial")] + [
        (item["after_seconds"], item["phase"]) for item in trace["transitions"]]
    failures = []
    # This allowance tests recovery, not seamless transport or clock precision.
    # Full traces retain the excluded transition interval for separate analysis.
    settle_seconds = .5
    for index, (start, name) in enumerate(transitions):
        end = transitions[index + 1][0] if index + 1 < len(transitions) else 28
        samples = [s for s in trace["samples"]
                   if s["phase"] == name and start + settle_seconds < s["elapsed_seconds"] < end - .1]
        captured = [w for w in windows if start + settle_seconds < w["seconds"] < end - .1]
        silent = sum(w["rms"] <= .01 for w in captured)
        phases[name]["settled_capture_windows"] = len(captured)
        phases[name]["settled_silent_windows"] = silent
        if not samples or not captured:
            failures.append(f"{name}: missing settled observations")
            continue
        if name == "unloaded":
            if silent != len(captured) or any(s["current-ao"] or s["path"] for s in samples):
                failures.append("unloaded: output or media remains active")
        else:
            if silent or any(not s["current-ao"] for s in samples):
                failures.append(f"{name}: audio did not remain available after settling")
            positions = {s["audio-pts"] for s in samples if s["audio-pts"] is not None}
            if len(positions) < 5:
                failures.append(f"{name}: clock did not advance")
    if trace.get("capture_exit") != 0:
        failures.append("capture process failed")
    return {"case": trace["case"], "capture_exit": trace.get("capture_exit"),
            "phases": phases, "recovery_check": {
                "settle_seconds": settle_seconds, "window_seconds": window_seconds,
                "failures": failures, "passed": not failures}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--mpv")
    source.add_argument("--analyze", type=Path, nargs="+", help="Recheck saved lifecycle traces")
    parser.add_argument("--capture", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path("tests/fixtures/generated"))
    args = parser.parse_args()
    observer = load("loop_probe", "audio-loop-probe.py")
    continuation = load("continuation_probe", "audio-continuation-probe.py")
    if args.analyze:
        failed = False
        for path in args.analyze:
            summary = summarize(json.loads(path.read_text()), continuation.capture_windows)
            failed |= "error" in summary or not summary["recovery_check"]["passed"]
            print(json.dumps({**summary, "trace_path": str(path)}), flush=True)
        return 1 if failed else 0
    if args.capture is None:
        parser.error("--mpv requires --capture")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix="audio-clock-lifecycle-", dir=args.output_dir)).resolve()
    media = folder / "first.wav"; replacement = folder / "replacement.wav"
    continuation.make_fixture(media)
    # Identical PCM isolates session reset from decoder/content differences;
    # a distinct path and distinct loop range identify the replacement.
    shutil.copy(media, replacement)
    replace = [["set_property", "pause", True], ["set_property", "ab-loop-a", "no"],
        ["set_property", "ab-loop-b", "no"], ["diagnostic-loadfile-and-wait", str(replacement)],
        ["set_property", "ab-loop-a", 2], ["set_property", "ab-loop-b", 2.323],
        ["seek", 2, "absolute+exact"], ["set_property", "pause", False]]
    actions = [
        (3, "half-speed", [["set_property", "speed", .5]]),
        (6, "fast", [["set_property", "speed", 1.5]]),
        (9, "normal", [["set_property", "speed", 1]]),
        (12, "stereo-output", [["set_property", "audio-channels", "stereo"]]),
        (15, "mono-output", [["set_property", "audio-channels", "mono"]]),
        (18, "replacement", replace),
        (22, "unloaded", [["stop"]]),
        (24, "reopened", replace),
    ]
    failed = False
    for name, options in [("default", []), ("stereo", ["--audio-channels=stereo"])]:
        trace = observer.observe(args.mpv, media, name, ["--idle=yes", *options],
            start=1.013, end=1.336, duration=28, actions=actions,
            properties=("speed", "current-ao", "path", "idle-active"),
            log_path=folder / f"{name}.mpv.log",
            capture=args.capture.resolve(strict=True), capture_path=folder / f"{name}.f32")
        path = folder / f"{name}.json"; path.write_text(json.dumps(trace) + "\n")
        summary = summarize(trace, continuation.capture_windows)
        failed |= "error" in summary or not summary["recovery_check"]["passed"]
        print(json.dumps({**summary, "trace_path": str(path)}), flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
