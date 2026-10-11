#!/usr/bin/env python3
"""Compare a known fixture pulse at the process-output tap with mpv clocks.

The phase-map results test a candidate interpretation; they are not a production
clock correction or a claim to measure acoustic output beyond the hardware tap.
Host timestamps and Python monotonic time use mach_absolute_time on macOS.
"""
import argparse
from array import array
from bisect import bisect_right
import json
from pathlib import Path
import statistics


def quantile(values, fraction):
    return sorted(values)[int(fraction * (len(values) - 1))]


def captured_pulses(trace):
    if "error" in trace:
        raise ValueError(trace["error"])
    path = Path(trace["capture_path"])
    metadata = json.loads(Path(str(path) + ".json").read_text())
    samples = array("f")
    samples.frombytes(path.read_bytes())
    if metadata["channels"] != 2 or len(samples) != metadata["frames"] * 2:
        raise ValueError("capture length or channel count mismatch")
    rate = metadata["sample_rate"]
    blocks = metadata["blocks"]
    starts = [block["first_frame"] for block in blocks]
    tick_seconds = metadata["host_time_numer"] / metadata["host_time_denom"] / 1e9
    onsets = []
    last_above_frame = None
    # The fixture pulse is 20 ms. Time stretching can split it into several
    # lobes; group by the intervening quiet gap, not by distance from its onset.
    quiet_gap_frames = rate * (0.02 / trace["speed"] + 0.02)
    for frame in range(metadata["frames"]):
        above = max(abs(samples[2 * frame]), abs(samples[2 * frame + 1])) > 0.01
        if above and (last_above_frame is None or frame - last_above_frame > quiet_gap_frames):
            block = blocks[bisect_right(starts, frame) - 1]
            if not block["flags"] & 2:  # kAudioTimeStampHostTimeValid
                raise ValueError("capture has no valid host timestamp")
            wall = block["host_ticks"] * tick_seconds + (frame - block["first_frame"]) / rate
            onsets.append(wall)
        if above:
            last_above_frame = frame
    return onsets, max(abs(sample) for sample in samples)


def analyze(trace, pulse_seconds):
    if not trace["looping"] or trace.get("transitions"):
        raise ValueError("phase comparison requires a steady looping trace")
    onsets, peak = captured_pulses(trace)
    result = {"case": trace["case"], "pulse_count": len(onsets), "peak_amplitude": peak}
    if len(onsets) < 3:
        result["error"] = "too few captured pulses to establish loop timing"
        return result
    length = trace["end"] - trace["start"]
    speed = trace["speed"]
    if not trace["start"] <= pulse_seconds < trace["end"]:
        raise ValueError("reference pulse must fall inside the loop")
    periods = [b - a for a, b in zip(onsets, onsets[1:])]
    result["expected_period_seconds"] = length / speed
    result["measured_period_median_seconds"] = statistics.median(periods)
    result["measured_period_min_seconds"] = min(periods)
    result["measured_period_max_seconds"] = max(periods)
    result["clock_comparisons"] = {}
    for property_name in ("time-pos", "audio-pts"):
        raw_errors, mapped_errors = [], []
        outside_range = 0
        for sample in trace["samples"]:
            wall = trace["sample_origin_monotonic_seconds"] + sample["elapsed_seconds"]
            value = sample[property_name]
            # Evaluate steady playback between measured pulses, without fitting
            # any arbitrary offset between the trace and the capture.
            if value is None or wall < onsets[1] or wall > onsets[-1]:
                continue
            nearest = min(onsets, key=lambda pulse: abs(pulse - wall))
            expected = trace["start"] + (pulse_seconds - trace["start"] +
                                          (wall - nearest) * speed) % length
            mapped = trace["start"] + (value - trace["start"]) % length
            raw_errors.append(value - expected)
            outside_range += not trace["start"] <= value < trace["end"]
            mapped_errors.append((mapped - expected + length / 2) % length - length / 2)
        if not mapped_errors:
            raise ValueError("no clock observations overlap the pulse capture")
        result["clock_comparisons"][property_name] = {
            "observations": len(mapped_errors),
            "raw_error_median_seconds": statistics.median(raw_errors),
            "raw_error_p95_seconds": quantile([abs(e) for e in raw_errors], 0.95),
            "raw_observations_outside_range": outside_range,
            "candidate_phase_error_median_seconds": statistics.median(mapped_errors),
            "candidate_phase_error_p95_seconds": quantile([abs(e) for e in mapped_errors], 0.95),
            "candidate_phase_error_max_seconds": max(abs(e) for e in mapped_errors),
        }
    return result


def analyze_transitions(trace):
    """Report raw observations by phase, without asserting a corrected clock."""
    if not trace.get("transitions"):
        raise ValueError("transition analysis requires a transition trace")
    onsets, peak = captured_pulses(trace)
    relative = [onset - trace["sample_origin_monotonic_seconds"] for onset in onsets]
    phases = [{"phase": "initial", "after_seconds": 0}, *trace["transitions"]]
    result = {"case": trace["case"], "pulse_count": len(onsets), "peak_amplitude": peak, "phases": []}
    for index, phase in enumerate(phases):
        start = phase["after_seconds"]
        end = phases[index + 1]["after_seconds"] if index + 1 < len(phases) else float("inf")
        samples = [s for s in trace["samples"] if s["phase"] == phase["phase"]]
        pulses = [p - start for p in relative if start <= p < end]
        values = [s["audio-pts"] for s in samples if s["audio-pts"] is not None]
        first_eof = next((s["elapsed_seconds"] for s in samples if s["eof-reached"]), None)
        result["phases"].append({
            "phase": phase["phase"], "after_seconds": start,
            "pulse_onsets_after_phase_seconds": pulses,
            "audio_pts_range_seconds": [min(values), max(values)] if values else None,
            "paused_observed": sorted(set(s["pause"] for s in samples if s["pause"] is not None)),
            "eof_observed": sorted(set(s["eof-reached"] for s in samples if s["eof-reached"] is not None)),
            "first_eof_seconds": first_eof,
            "pulse_onsets_after_eof": sum(p + start > first_eof for p in pulses)
                if first_eof is not None else None,
        })
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--pulse", type=float, default=7199.0)
    parser.add_argument("--transitions", action="store_true", help="Report transition phases without a phase-map accuracy claim")
    args = parser.parse_args()
    failed = False
    for line in args.trace.read_text().splitlines():
        trace = json.loads(line)
        try:
            result = analyze_transitions(trace) if args.transitions else analyze(trace, args.pulse)
        except (ValueError, OSError, KeyError) as error:
            result = {"case": trace.get("case"), "error": str(error)}
        print(json.dumps(result), flush=True)
        failed |= "error" in result
    raise SystemExit(1 if failed else 0)
