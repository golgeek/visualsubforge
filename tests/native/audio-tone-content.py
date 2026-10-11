"""Shared coarse tone-content oracle; deliberately makes no presentation-time claim."""
from array import array
import json
import math
from pathlib import Path
import sys


def analyze_content(path, capture_kind):
    """Check the known three-tone fixture's captured sequence, not its timing."""
    meta = json.loads(Path(str(path) + ".json").read_text())
    pcm = array("f")
    pcm.frombytes(path.read_bytes())
    if sys.byteorder != "little":
        pcm.byteswap()
    if (meta["capture_kind"] != capture_kind or meta["format"] != "float32le"
            or meta["sample_rate"] != 48000 or meta["channels"] != 2
            or len(pcm) != 2 * meta["frames"] or not pcm):
        raise ValueError("invalid capture metadata/length")
    count = 960  # 20 ms windows, coarse content classification only.
    tones = {440: 0, 660: 0, 880: 0}
    sequence = []
    unknown = 0
    for start in range(0, len(pcm) - 2 * count + 1, 2 * count):
        values = pcm[start:start + 2 * count:2]
        rms = math.sqrt(sum(v * v for v in values) / count)
        if rms <= .01:
            continue
        crossings = [i - a / (b - a) for i, (a, b) in enumerate(zip(values, values[1:]))
                     if a <= 0 < b]
        hz = ((len(crossings) - 1) * 48000 / (crossings[-1] - crossings[0])
              if len(crossings) > 2 else 0)
        tone = min(tones, key=lambda t: abs(t - hz))
        if abs(tone - hz) >= 10:
            unknown += 1
            continue
        tones[tone] += 1
        if not sequence or sequence[-1] != tone:
            sequence.append(tone)
    checks = {"tone_order": sequence == [440, 660, 880],
              "tone_durations": tones[440] >= 25 and tones[660] >= 40 and tones[880] >= 50,
              "limited_unclassified_windows": unknown <= 5}
    return {"passed": all(checks.values()), "checks": checks, "tone_sequence_hz": sequence,
            "tone_seconds": {str(t): n * .02 for t, n in tones.items()},
            "unclassified_windows": unknown, "window_seconds": .02,
            "timing_basis": "PCM sample count only; no presentation-time assertion"}
