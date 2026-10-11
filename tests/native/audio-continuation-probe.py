#!/usr/bin/env python3
"""Capture distinguishable audio after clearing a loop and seeking to raw PTS.

Diagnostic only: capture is before the DAC; timing observations are not acoustic
latency or application acceptance. Generates its own four-second mono fixture.
"""
import argparse
from array import array
from bisect import bisect_right
import importlib.util
import json
import math
from pathlib import Path
import statistics
import tempfile
import wave


def make_fixture(path):
    rate = 48000
    samples = array("h")
    phase = 0.0
    for frame in range(4 * rate):
        seconds = frame / rate
        frequency = 440 if seconds < 1.5 else 660 if seconds < 2.5 else 880
        samples.append(round(8000 * math.sin(phase)))
        phase = (phase + 2 * math.pi * frequency / rate) % (2 * math.pi)
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(rate)
        output.writeframes(samples.tobytes())


def capture_windows(trace, window_seconds=0.02):
    if "error" in trace:
        raise ValueError(trace["error"])
    path = Path(trace["capture_path"])
    meta = json.loads(Path(str(path) + ".json").read_text())
    pcm = array("f")
    pcm.frombytes(path.read_bytes())
    if meta["channels"] != 2 or len(pcm) != 2 * meta["frames"]:
        raise ValueError("invalid capture length/channels")
    rate = meta["sample_rate"]
    blocks = meta["blocks"]
    starts = [b["first_frame"] for b in blocks]
    tick = meta["host_time_numer"] / meta["host_time_denom"] / 1e9

    def timestamp(frame):
        block = blocks[bisect_right(starts, frame) - 1]
        if not block["flags"] & 2:
            raise ValueError("missing capture host timestamp")
        return (block["host_ticks"] * tick + (frame - block["first_frame"]) / rate
                - trace["sample_origin_monotonic_seconds"])

    windows = []
    count = round(rate * window_seconds)
    if count < 1:
        raise ValueError("analysis window must contain at least one sample")
    for start in range(0, meta["frames"] - count + 1, count):
        values = pcm[2 * start:2 * (start + count):2]
        rms = math.sqrt(sum(v * v for v in values) / count)
        crossings = [i - a / (b - a) for i, (a, b) in enumerate(zip(values, values[1:]))
                     if a <= 0 < b]
        frequency = ((len(crossings) - 1) * rate / (crossings[-1] - crossings[0])
                     if rms > 0.01 and len(crossings) > 2 else None)
        windows.append({"seconds": timestamp(start + count / 2),
                        "rms": rms, "frequency_hz": frequency})
    return windows, count / rate


def measure(trace):
    windows, window_seconds = capture_windows(trace)
    transition = trace["transitions"][0]
    cleared = transition["after_seconds"]
    audible = [w for w in windows if w["seconds"] > cleared and w["rms"] > 0.01]
    eof = next((s["elapsed_seconds"] for s in trace["samples"]
                if s["elapsed_seconds"] > cleared and s["eof-reached"]), None)
    tones = {}
    for frequency in (440, 660, 880):
        matches = [w["seconds"] for w in audible
                   if w["frequency_hz"] and abs(w["frequency_hz"] - frequency) < 10]
        tones[str(frequency)] = [min(matches), max(matches)] if matches else None
    valid = [s for s in trace["samples"] if s["elapsed_seconds"] > cleared
             and s["audio-pts"] is not None and not s["seeking"]]
    clock_errors = []
    for w in audible:
        frequency = w["frequency_hz"]
        if not frequency or not valid:
            continue
        sample = min(valid, key=lambda s: abs(s["elapsed_seconds"] - w["seconds"]))
        if abs(sample["elapsed_seconds"] - w["seconds"]) > 0.04:
            continue
        # Tone changes give coarse, independently audible source intervals.
        low, high = (0, 1.5) if abs(frequency - 440) < 10 else (
            (1.5, 2.5) if abs(frequency - 660) < 10 else
            (2.5, 4) if abs(frequency - 880) < 10 else (None, None))
        if low is not None:
            clock_errors.append(max(low - sample["audio-pts"], sample["audio-pts"] - high, 0))
    gaps = [w["seconds"] for w in windows
            if audible and audible[0]["seconds"] < w["seconds"] < audible[-1]["seconds"]
            and w["rms"] <= 0.01]
    # Short RMS windows locate interruptions more precisely than the tone
    # estimator. Start before the clear command so an immediate interruption
    # is not missed; stop at the last audible window to exclude the EOF tail.
    fine, fine_step = capture_windows(trace, 0.002)
    fine_audible = [w for w in fine if w["seconds"] > cleared and w["rms"] > 0.01]
    before_clear = [w for w in fine
                    if w["seconds"] < transition["before_seconds"] and w["rms"] > 0.01]
    intervals = []
    if fine_audible and before_clear:
        for w in fine:
            if not (before_clear[-1]["seconds"] < w["seconds"] < fine_audible[-1]["seconds"]):
                continue
            if w["rms"] > 0.01:
                continue
            start, end = w["seconds"] - fine_step / 2, w["seconds"] + fine_step / 2
            if intervals and abs(intervals[-1][1] - start) < fine_step / 10:
                intervals[-1][1] = end
            else:
                intervals.append([start, end])
    return {"case": trace["case"], "clear_seconds": cleared,
            "transition_duration_seconds": cleared - transition["before_seconds"],
            "seek_target_seconds": transition["resolved_seek_seconds"],
            "tones_host_seconds": tones, "first_eof_seconds": eof,
            "last_audible_window_seconds": audible[-1]["seconds"] if audible else None,
            "silent_windows_within_continuation": gaps,
            "silent_intervals_seconds": intervals,
            "gap_window_seconds": fine_step,
            "gap_analysis_has_pre_clear_audio": bool(before_clear),
            "silence_rms_threshold": 0.01,
            "clock_outside_tone_interval_max_seconds": max(clock_errors) if clock_errors else None,
            "clock_outside_tone_interval_median_seconds": statistics.median(clock_errors) if clock_errors else None,
            "analysis_window_seconds": window_seconds}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--mpv", help="Capture using this player executable")
    mode.add_argument("--analyze", type=Path, nargs="+", help="Reanalyze existing trace JSON files")
    parser.add_argument("--capture", type=Path, help="Process capture helper; required with --mpv")
    parser.add_argument("--transport-matrix", action="store_true",
                        help="Compare paused loop-clear with ordinary paused refresh seek")
    parser.add_argument("--output-dir", type=Path, default=Path("tests/fixtures/generated"))
    args = parser.parse_args()
    if args.analyze:
        failed = False
        for path in args.analyze:
            try:
                result = measure(json.loads(path.read_text()))
            except (ValueError, KeyError, OSError) as error:
                failed = True
                result = {"error": str(error)}
            print(json.dumps({**result, "trace_path": str(path)}), flush=True)
        raise SystemExit(1 if failed else 0)
    if args.capture is None:
        parser.error("--capture is required with --mpv")
    spec = importlib.util.spec_from_file_location("loop_probe", Path(__file__).with_name("audio-loop-probe.py"))
    probe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(probe)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="audio-continuation-", dir=args.output_dir)).resolve()
    fixture = directory / "tones.wav"
    make_fixture(fixture)
    failed = False
    cases = []
    for output, options in (("default", []), ("stereo", ["--audio-channels=stereo"])):
        if args.transport_matrix:
            for looping, operation in ((True, "clear-loop"), (False, "refresh")):
                cases.append((f"{output}-{operation}", options, looping, operation))
        else:
            cases.append((output, options, True, "legacy-clear"))
    for name, options, looping, operation in cases:
        paused_transport = operation != "legacy-clear"
        action = ("diagnostic-refresh-at-audio-pts" if operation == "refresh"
                  else "diagnostic-clear-loop-at-audio-pts")
        commands = [[action]]
        if paused_transport:
            commands = [["set_property", "pause", True], *commands,
                        ["set_property", "pause", False]]
        trace = probe.observe(args.mpv, fixture, name, options,
                              start=1.013 if looping else 0.013, end=1.336,
                              looping=looping, duration=8 if looping else 6,
                              capture=args.capture.resolve(strict=True),
                              capture_path=directory / f"{name}.f32",
                              actions=[(3 if looping else 1.2, operation, commands)])
        trace_path = directory / f"{name}.json"
        trace_path.write_text(json.dumps(trace) + "\n")
        try:
            result = measure(trace)
            if args.transport_matrix:
                # This checks command responsiveness and eventual continuation,
                # not seamless output or an acoustic latency guarantee.
                checks = {
                    "transition_under_250ms": result["transition_duration_seconds"] < 0.25,
                    "pre_transition_audio": result["gap_analysis_has_pre_clear_audio"],
                    "all_three_tones_continue": all(result["tones_host_seconds"].values()),
                    "eof_after_audio": result["first_eof_seconds"] is not None
                        and result["last_audible_window_seconds"] is not None
                        and result["first_eof_seconds"] >= result["last_audible_window_seconds"],
                }
                result["transport_checks"] = checks
                failed |= not all(checks.values())
        except (ValueError, KeyError, OSError) as error:
            failed = True
            result = {"case": name, "error": str(error)}
        print(json.dumps({**result, "trace_path": str(trace_path)}), flush=True)
    raise SystemExit(1 if failed else 0)
