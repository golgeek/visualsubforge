#!/usr/bin/env python3
"""Inspect Windows process-loopback packet timing and coarse tone continuation.

QPC-to-mpv alignment uses a bracketed helper invocation by default. An opt-in
native diagnostic can bracket mp_time_ns directly with QPC reads at both ends
of playback; its interval must also agree with the helper measurement.
Results describe capture before the physical speakers, not acoustic latency.
Usage: python windows-capture-analysis.py RESULT_DIRECTORY [--native-clock]
"""
from array import array
from bisect import bisect_right
import json
import math
from pathlib import Path
import re
import statistics
import sys


def native_clock_interval(log):
    """Intersect QPC brackets around mp_time_ns in an opt-in native diagnostic.

    This aligns two reads of QPC, not the audio-device clock. Keep the full
    measured uncertainty and reject contradictory observations.
    """
    intervals = []
    phases = set()
    for line in log.splitlines():
        if "VSF_CLOCK_BRACKET" not in line:
            continue
        values = dict(re.findall(r"(\w+)=(-?\d+)", line))
        before, after, frequency, mp_ns = (
            int(values[key]) for key in ("qpc_before", "qpc_after", "frequency", "mp_ns"))
        if before < 0 or after < before or frequency <= 0 or mp_ns < 0:
            raise ValueError("invalid native clock bracket")
        phases.add(int(values["phase"]))
        # The native QPC-to-nanosecond conversion truncates by less than 1 ns.
        intervals.append((before / frequency - mp_ns / 1e9 - 1e-9,
                          after / frequency - mp_ns / 1e9 + 1e-9))
    if len(intervals) < 2 or phases != {0, 1}:
        raise ValueError("need native clock brackets at initialization and teardown")
    low, high = max(v[0] for v in intervals), min(v[1] for v in intervals)
    if low > high:
        raise ValueError("native clock brackets disagree")
    return low, high, len(intervals)


def measure(folder, native_clock=False):
    trace = json.loads((folder / "trace.json").read_text())
    meta = json.loads((folder / "output.f32.json").read_text())
    if trace.get("error") or trace.get("capture_exit") != 0:
        raise ValueError("capture/player failed")
    if (meta["capture_kind"] != "windows-process-loopback"
            or meta["format"] != "float32le" or meta["sample_rate"] != 48000
            or meta["channels"] != 2):
        raise ValueError("unexpected capture format")
    pcm = array("f")
    pcm.frombytes((folder / "output.f32").read_bytes())
    if sys.byteorder != "little":
        pcm.byteswap()
    if len(pcm) != meta["frames"] * 2 or not pcm:
        raise ValueError("capture length mismatch")
    packets = meta["packets"]
    consumed = 0
    for packet in packets:
        if packet["frame_offset"] != consumed or packet["frames"] <= 0:
            raise ValueError("packet offsets do not cover PCM consecutively")
        if packet["flags"] & 4:  # AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR
            raise ValueError("capture reports an invalid timestamp")
        consumed += packet["frames"]
    if consumed != meta["frames"]:
        raise ValueError("packet frame count mismatch")
    calibration = trace["capture_clock_calibration"]
    before, after = calibration["mp_before"], calibration["mp_after"]
    if after < before or calibration["qpc_frequency"] <= 0:
        raise ValueError("invalid clock calibration bracket")
    qpc = calibration["qpc_ticks"] / calibration["qpc_frequency"]
    offset_low, offset_high = qpc - after, qpc - before
    legacy_bracket = after - before
    native_count = 0
    if native_clock:
        native_low, native_high, native_count = native_clock_interval(
            (folder / "native.log").read_text(encoding="utf-8"))
        if max(offset_low, native_low) > min(offset_high, native_high):
            raise ValueError("native and subprocess clock brackets disagree")
        offset_low, offset_high = native_low, native_high
    alignment_width = offset_high - offset_low
    offset_mid = (offset_low + offset_high) / 2
    origin = trace["sample_origin_mp_time_seconds"]
    starts = [p["frame_offset"] for p in packets]

    def timestamp(frame):
        packet = packets[bisect_right(starts, frame) - 1]
        return (packet["qpc_100ns"] / 1e7 + (frame - packet["frame_offset"]) / 48000
                - offset_mid - origin)

    def windows(count, classify):
        result = []
        for start in range(0, meta["frames"] - count + 1, count):
            values = pcm[2 * start:2 * (start + count):2]
            if not all(math.isfinite(v) for v in values):
                raise ValueError("nonfinite PCM sample")
            rms = math.sqrt(sum(v * v for v in values) / count)
            hz = None
            if classify and rms > .01:
                crossings = [i - a / (b - a) for i, (a, b) in enumerate(zip(values, values[1:]))
                             if a <= 0 < b]
                if len(crossings) > 2:
                    hz = (len(crossings) - 1) * 48000 / (crossings[-1] - crossings[0])
            result.append({"seconds": timestamp(start + count / 2), "rms": rms, "hz": hz})
        return result

    transition = next((t for t in trace["transitions"] if t["phase"] in ("cleared", "refreshed")), None)
    if transition is None:
        if trace["transitions"] or trace["looping"]:
            raise ValueError("no supported transition in this trace")
        transition = {"phase": "initial", "before_seconds": 0, "after_seconds": 0,
                      "resolved_seek_seconds": trace["start"],
                      "reference": "observation origin after initial unpause, not seek completion"}
    coarse = windows(960, True)
    audible = [w for w in coarse if w["seconds"] > transition["after_seconds"] and w["rms"] > .01]
    tones = {}
    for hz in (440, 660, 880):
        times = [w["seconds"] for w in audible if w["hz"] and abs(w["hz"] - hz) < 10]
        tones[str(hz)] = [min(times), max(times)] if times else None
    marker_residual = None
    if tones["440"] and tones["660"] and transition["resolved_seek_seconds"] < 1.5:
        # Compare the captured 1.5 s marker against an explicitly hypothetical
        # zero-delay seek. Include classification-window and alignment bounds;
        # do not label this residual device latency or a corrected media clock.
        nominal = transition["after_seconds"] + 1.5 - transition["resolved_seek_seconds"]
        uncertainty = .01 + alignment_width / 2
        marker_residual = [tones["440"][1] - uncertainty - nominal,
                           tones["660"][0] + uncertainty - nominal]
    clock_markers = []
    samples = trace["samples"]
    for sample in samples:
        completed = sample.get("sample_complete_elapsed_seconds")
        if completed is not None and (not math.isfinite(completed)
                                      or completed < sample["elapsed_seconds"]):
            raise ValueError("invalid property-read completion bracket")
    for marker, old_tone, new_tone in [(1.5, "440", "660"), (2.5, "660", "880")]:
        if not tones[old_tone] or not tones[new_tone]:
            continue
        uncertainty = .01 + alignment_width / 2
        captured = [tones[old_tone][1] - uncertainty, tones[new_tone][0] + uncertainty]
        for index, (a, b) in enumerate(zip(samples, samples[1:])):
            if (a["phase"] != transition["phase"] or b["phase"] != a["phase"]
                    or a["audio-pts"] is None or b["audio-pts"] is None
                    or a["seeking"] or b["seeking"]
                    or not a["audio-pts"] < marker <= b["audio-pts"]):
                continue
            # New observers bracket the property reads directly. Old traces
            # conservatively use the following callback as the completion bound.
            completed = b.get("sample_complete_elapsed_seconds")
            if completed is None:
                if index + 2 >= len(samples) or samples[index + 2]["phase"] != b["phase"]:
                    continue
                completed = samples[index + 2]["elapsed_seconds"]
            # mp.get_time uses mpv_get_time_us: account for its truncation at
            # the upper bound, even when native QPC alignment is much tighter.
            reported = [a["elapsed_seconds"], completed + 1e-6]
            clock_markers.append({"source_marker_seconds": marker,
                                  "reported_clock_crossing_bracket_seconds": reported,
                                  "capture_marker_bracket_seconds": captured,
                                  "capture_minus_reported_clock_seconds":
                                      [captured[0] - reported[1], captured[1] - reported[0]]})
            break
    fine = windows(96, False)
    pre = [w for w in fine if transition["before_seconds"] - .25 < w["seconds"] < transition["before_seconds"] and w["rms"] > .01]
    post = [w for w in fine if w["seconds"] > transition["after_seconds"] and w["rms"] > .01]
    quiet = []
    if pre and post:
        for w in fine:
            if pre[-1]["seconds"] < w["seconds"] < post[-1]["seconds"] and w["rms"] <= .01:
                start, end = w["seconds"] - .001, w["seconds"] + .001
                if quiet and abs(quiet[-1][1] - start) < .0002:
                    quiet[-1][1] = end
                else:
                    quiet.append([start, end])
    timestamp_steps = []
    for a, b in zip(packets, packets[1:]):
        delta = (b["qpc_100ns"] - a["qpc_100ns"]) / 1e7 - a["frames"] / 48000
        if abs(delta) > .002:
            timestamp_steps.append({"at_seconds": timestamp(b["frame_offset"]),
                                    "difference_from_pcm_seconds": delta})
    return {"capture_kind": meta["capture_kind"], "frames": meta["frames"],
            "calibration_bracket_seconds": alignment_width,
            "subprocess_calibration_bracket_seconds": legacy_bracket,
            "native_clock_brackets": native_count,
            "lua_clock_resolution_seconds": 1e-6,
            "timestamp_alignment_half_width_seconds": alignment_width / 2,
            "timestamp_basis": "GetBuffer QPC, midpoint within " +
                ("native QPC/mp_time brackets" if native_clock else "subprocess calibration bracket"),
            "device_positions_all_zero": all(p["device_frame"] == 0 for p in packets),
            "discontinuity_flag_packets": sum(bool(p["flags"] & 1) for p in packets),
            "packet_timestamp_steps_over_2ms": timestamp_steps,
            "transition": transition, "tone_window_centers_seconds": tones,
            "captured_marker_minus_zero_delay_seek_prediction_seconds": marker_residual,
            "clock_marker_comparisons": clock_markers,
            "quiet_intervals_seconds": quiet, "quiet_window_seconds": .002,
            "rms_threshold": .01, "has_pre_transition_audio": bool(pre),
            "last_audible_center_seconds": post[-1]["seconds"] if post else None,
            "sample_interval_median_seconds": statistics.median(
                b["elapsed_seconds"] - a["elapsed_seconds"]
                for a, b in zip(trace["samples"], trace["samples"][1:])),
            "limits": "No acoustic/device latency assertion; packet timestamp gaps are retained, not filled."}


if __name__ == "__main__":
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != "--native-clock"):
        raise SystemExit("usage: windows-capture-analysis.py RESULT_DIRECTORY [--native-clock]")
    print(json.dumps(measure(Path(sys.argv[1]), native_clock=len(sys.argv) == 3), indent=2))
