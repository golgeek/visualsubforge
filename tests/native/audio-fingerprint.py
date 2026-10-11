#!/usr/bin/env python3
"""Reject gross content discontinuities in ordinary/pause process captures.

Requires NumPy. Input is unique 48 kHz PCM16 WAV and process-audio-capture's
stereo float32 output/JSON. This is not a loop/seek oracle or an acoustic test.
Exit 0 means sufficient correspondence and no detected gross discontinuity,
not sample identity. Unmatched beginnings/endings and ambiguous windows are
reported explicitly. The conservative two-window allowance is not a product
precision budget. See mpv-clock/README.md.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import wave

import numpy as np

WINDOW = 256
HOP = 64
MIN_CORRELATION = .97
MIN_MARGIN = .10
MIN_MATCHED_WINDOWS = 500


def match(reference, captured, *, fractional=False):
    """Locate unique windows; constants retain the earlier EOF experiment."""
    if min(len(reference), len(captured)) < WINDOW:
        raise ValueError("reference and capture must contain a full window")
    if not np.isfinite(reference).all() or not np.isfinite(captured).all():
        raise ValueError("nonfinite audio samples")
    nfft = 1 << (len(reference) + WINDOW - 2).bit_length()
    spectrum = np.fft.rfft(reference, nfft)
    sums = np.r_[0, np.cumsum(reference)]
    squares = np.r_[0, np.cumsum(reference * reference)]
    ref_sum = sums[WINDOW:] - sums[:-WINDOW]
    ref_energy = squares[WINDOW:] - squares[:-WINDOW] - ref_sum**2 / WINDOW
    # Correlation is undefined for a constant reference window. FFT roundoff
    # divided by the tiny denominator below can otherwise make silence win
    # over the actual signal. Count transitions directly: cumulative variance
    # alone can also acquire a small positive error for a constant window.
    transitions = np.r_[0, np.cumsum(reference[1:] != reference[:-1])]
    varying = transitions[WINDOW - 1:] != transitions[:-(WINDOW - 1)]
    valid_reference = varying & (ref_energy > 0)
    if fractional:
        # A sub-sample capture offset can lower integer-only correlation even
        # when content is intact. Fit a positive combination of adjacent source
        # windows, i.e. gain * ((1-phase)*x + phase*y), with 0 <= phase <= 1.
        # This adds no time-warping, per-sample fitting or threshold relaxation.
        products = np.r_[0, np.cumsum(reference[:-1] * reference[1:])]
        covariance = products[WINDOW:] - products[:-WINDOW] - ref_sum[:-1] * ref_sum[1:] / WINDOW
        determinant = ref_energy[:-1] * ref_energy[1:] - covariance**2
        valid_energy = (determinant > 1e-20) & valid_reference[:-1] & valid_reference[1:]
        denominator = np.maximum(determinant, 1e-20)
        inverse_x = ref_energy[1:] / denominator
        inverse_y = ref_energy[:-1] / denominator
        inverse_xy = covariance / denominator
    rows = []
    for start in range(0, len(captured) - WINDOW + 1, HOP):
        chunk = captured[start:start + WINDOW]
        rms = float(np.sqrt(np.mean(chunk * chunk)))
        if rms < .001:
            rows.append(dict(capture_frame=start, kind="quiet", rms=rms))
            continue
        centered = chunk - chunk.mean()
        dots = np.fft.irfft(
            spectrum * np.conj(np.fft.rfft(centered, nfft)), nfft
        )[:len(ref_energy)]
        scores = dots / np.sqrt(np.maximum(
            ref_energy * np.dot(centered, centered), 1e-30))
        scores = np.where(valid_reference, scores, -1)
        phases = None
        if fractional:
            weight_x = inverse_x * dots[:-1] - inverse_xy * dots[1:]
            weight_y = inverse_y * dots[1:] - inverse_xy * dots[:-1]
            valid = valid_energy & (weight_x >= 0) & (weight_y >= 0)
            explained = weight_x * dots[:-1] + weight_y * dots[1:]
            fitted_scores = np.sqrt(np.clip(explained / max(np.dot(centered, centered), 1e-30), 0, 1))
            better = valid & (fitted_scores > scores[:-1])
            scores[:-1] = np.where(better, fitted_scores, scores[:-1])
            phases = np.r_[np.where(better, weight_y / np.maximum(weight_x + weight_y, 1e-30), 0), 0]
        best = int(np.argmax(scores))
        score = float(scores[best])
        rivals = scores.copy()
        rivals[max(0, best - 32):best + 33] = -1
        second = float(np.max(rivals))
        kind = ("matched" if score >= MIN_CORRELATION
                and score - second >= MIN_MARGIN else "ambiguous")
        source_frame = best if phases is None else float(best + phases[best])
        rows.append(dict(capture_frame=start, source_frame=source_frame,
                         correlation=score, second_correlation=second,
                         rms=rms, kind=kind))
    return rows


def segments(rows):
    groups = []
    for row in rows:
        if row["kind"] != "matched":
            continue
        previous = groups[-1][-1] if groups else None
        continuous = previous is not None and (
            row["capture_frame"] - previous["capture_frame"] == HOP
            and abs(row["source_frame"] - previous["source_frame"] - HOP) <= 1)
        if continuous:
            groups[-1].append(row)
        else:
            groups.append([row])
    return [dict(capture_start=g[0]["capture_frame"],
                 capture_end=g[-1]["capture_frame"] + WINDOW,
                 source_start=g[0]["source_frame"],
                 source_end=g[-1]["source_frame"] + WINDOW,
                 windows=len(g)) for g in groups]


def discontinuities(groups, captured):
    """Check every mapping transition, including capture windows that overlap."""
    gaps = []
    for before, after in zip(groups, groups[1:]):
        first, last = before["capture_end"], after["capture_start"]
        capture_gap = last - first
        source_gap = after["source_start"] - before["source_end"]
        # Only exact zero runs of a full window are unavailable for source
        # content. All other unmatched capture samples get the benefit of doubt.
        zeros = captured[first:last] == 0 if last > first else np.array([], bool)
        edges = np.diff(np.r_[False, zeros, False].astype(int))
        lengths = np.flatnonzero(edges == -1) - np.flatnonzero(edges == 1)
        silent = int(sum(length for length in lengths if length >= WINDOW))
        available = max(0, capture_gap) - silent
        forward = max(0, source_gap - available - 2 * WINDOW)
        # Adjacent matched windows may overlap normally. Discount only that
        # overlap, not arbitrary backwards source jumps/replayed intervals.
        backward = max(0, -source_gap - max(0, -capture_gap) - 2 * WINDOW)
        gaps.append(dict(capture_gap_frames=capture_gap,
                         source_gap_frames=source_gap, silent_frames=silent,
                         unexplained_forward_frames=forward,
                         unexplained_backward_frames=backward))
    return gaps


def analyze(reference, captured):
    rows = match(reference, captured)
    groups = segments(rows)
    gaps = discontinuities(groups, captured)
    counts = {kind: sum(row["kind"] == kind for row in rows)
              for kind in ("matched", "quiet", "ambiguous")}
    failures = []
    if counts["matched"] < MIN_MATCHED_WINDOWS:
        failures.append("insufficient matched coverage")
    if any(gap["unexplained_forward_frames"] for gap in gaps):
        failures.append("unexplained forward source jump")
    if any(gap["unexplained_backward_frames"] for gap in gaps):
        failures.append("unexplained repeated source content")
    return dict(passed=not failures, failures=failures, counts=counts,
                first_matched_source_frame=groups[0]["source_start"] if groups else None,
                last_matched_source_frame=groups[-1]["source_end"] if groups else None,
                reference_frames=len(reference), segments=groups, gaps=gaps), rows


def validate_metadata(meta, frames):
    if meta["sample_rate"] != 48000 or meta["channels"] != 2:
        raise ValueError("requires 48 kHz stereo process capture")
    if meta["frames"] != frames or frames <= 0:
        raise ValueError("capture length differs from metadata or is empty")
    blocks = meta["blocks"]
    if not blocks or blocks[0]["first_frame"] != 0:
        raise ValueError("missing initial capture block")
    if not all(math.isfinite(meta[key]) and meta[key] > 0
               for key in ("host_time_numer", "host_time_denom")):
        raise ValueError("invalid host timebase")
    tick = meta["host_time_numer"] / meta["host_time_denom"] / 1e9
    for block in blocks:
        if block["flags"] & 3 != 3 or block["frames"] <= 0:
            raise ValueError("invalid capture timestamp or block size")
        if not all(math.isfinite(block[key]) for key in (
                "first_frame", "sample_time", "host_ticks", "frames")):
            raise ValueError("nonfinite capture timestamp")
    for before, after in zip(blocks, blocks[1:]):
        if (before["first_frame"] + before["frames"] != after["first_frame"]
                or before["sample_time"] + before["frames"] != after["sample_time"]
                or abs((after["host_ticks"] - before["host_ticks"]) * tick
                       - before["frames"] / 48000) >= 1 / 48000):
            raise ValueError("discontinuous capture timestamps")
    if blocks[-1]["first_frame"] + blocks[-1]["frames"] != frames:
        raise ValueError("capture blocks do not cover PCM")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--output", required=True, type=Path,
                        help="new JSON report path; existing files are never overwritten")
    args = parser.parse_args()
    with wave.open(str(args.reference), "rb") as source:
        channels = source.getnchannels()
        if channels not in (1, 2) or source.getsampwidth() != 2 or source.getframerate() != 48000:
            raise ValueError("requires mono/stereo 48 kHz PCM16 reference")
        reference = np.frombuffer(source.readframes(source.getnframes()), "<i2")
        reference = reference.astype(float).reshape(-1, channels) / 32768
    metadata = Path(str(args.capture) + ".json")
    pcm = np.fromfile(args.capture, "<f4").reshape(-1, 2).astype(float)
    validate_metadata(json.loads(metadata.read_text()), len(pcm))
    results = []
    for channel in range(2):
        summary, _ = analyze(reference[:, min(channel, channels - 1)], pcm[:, channel])
        results.append(dict(capture_channel=channel, **summary))
    report = dict(passed=all(result["passed"] for result in results),
                  scope="gross discontinuities within matched content; not acoustic or sample identity",
                  window_frames=WINDOW, hop_frames=HOP,
                  min_correlation=MIN_CORRELATION, min_margin=MIN_MARGIN,
                  min_matched_windows=MIN_MATCHED_WINDOWS,
                  input_sha256={str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest()
                                for path in (args.reference, args.capture, metadata, Path(__file__))},
                  channels=results)
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    print(json.dumps(dict(passed=report["passed"], report=str(args.output))))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
