#!/usr/bin/env python3
"""Check stretched PCM tails against a delayed-EOF reference, without a device.

The control renders the same input with one second of appended silence, moving
EOF beyond the tested endpoint. Matching its prefix checks actual tail samples,
not just output length. Both players share WSOLA: this is not an independent
perceptual-quality oracle or evidence of device/loop-transition continuity.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import wave


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_wave(path, data):
    with wave.open(str(path), "wb") as output:
        output.setparams((2, 2, 48000, 0, "NONE", "not compressed"))
        output.writeframes(data)


def compare(original, candidate, reference, frames, speed):
    """Preserve every old sample; validate newly emitted content independently.

    Extending the input can change a few final PCM16 rounding decisions even in
    the control. An unchanged candidate needs exact control equality. A longer
    candidate must additionally match the entire delayed-EOF reference prefix.
    """
    added = candidate[len(original):]
    duration_ok = abs(len(candidate) / 4 - frames / speed) <= 1
    preserves_control = len(candidate) >= len(original) and candidate[:len(original)] == original
    candidate_matches = candidate == reference[:len(candidate)]
    return {"duration_ok": duration_ok, "preserves_control": preserves_control,
            "control_matches_reference": original == reference[:len(original)],
            "candidate_matches_reference": candidate_matches,
            "added_frames": len(added) // 4, "added_tail_nonzero": any(added),
            "zero_padding_matches_reference": original + bytes(len(added)) == reference[:len(candidate)],
            "passed": bool(candidate) and len(candidate) % 4 == 0 and duration_ok
                      and preserves_control and (not added or candidate_matches)}


def render(player, source, speed, output):
    command = [str(player), "--no-config", "--vo=null", "--vid=no",
               "--no-audio-display", "--ao=pcm", "--ao-pcm-waveheader=no",
               f"--ao-pcm-file={output}", "--audio-format=s16",
               "--audio-channels=stereo", "--audio-samplerate=48000",
               "--volume=100", f"--speed={speed}", str(source)]
    # Every run selects its executable explicitly; inherited diagnostic/library
    # overrides must not quietly substitute the application's previous runtime.
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("VSF_", "DYLD_")) and k != "LD_LIBRARY_PATH"}
    forced = False
    with output.with_suffix(".log").open("xb") as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, env=env)
        try:
            code = process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            forced = True
            process.kill()
            code = process.wait()
    data = output.read_bytes() if output.exists() else b""
    return data, {"command": command, "exit": code, "forced": forced,
                  "source_sha256": sha(source), "output_bytes": len(data),
                  "output_sha256": hashlib.sha256(data).hexdigest(),
                  "passed": code == 0 and not forced and bool(data) and len(data) % 4 == 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", type=Path, required=True)
    parser.add_argument("--control", type=Path, required=True,
                        help="Same-base player without the EOF availability correction")
    parser.add_argument("--reference", type=Path, required=True,
                        help="At least two seconds of synthetic stereo 48 kHz PCM16 WAV")
    parser.add_argument("--output-dir", type=Path, required=True, help="New evidence directory")
    args = parser.parse_args()
    player, control, source = (p.resolve(strict=True) for p in
                               (args.mpv, args.control, args.reference))
    with wave.open(str(source), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (2, 2, 48000):
            parser.error("reference must be stereo 48 kHz PCM16 WAV")
        raw = wav.readframes(96000)
        if len(raw) != 96000 * 4:
            parser.error("reference must contain at least two seconds")
    root = args.output_dir.resolve()
    root.mkdir(parents=True, exist_ok=False)
    criteria = {"input_frames": [48, 960, 15504, 96000],
                "speeds": [.5, .75, 1, 1.25, 1.5, 2],
                "duration_tolerance_frames": 1,
                "reference": "Control with 48000 silence frames appended; compare prefix",
                "acceptance": "Exact control prefix preserved; extended candidate also matches entire reference prefix",
                "scope": "PCM EOF accounting/content; no device or loop-clear acceptance",
                "candidate_sha256": sha(player), "control_sha256": sha(control),
                "fixture_sha256": sha(source)}
    (root / "criteria.json").write_text(json.dumps(criteria, indent=2) + "\n")
    results = []
    for count in criteria["input_frames"]:
        short, extended = root / f"{count}.wav", root / f"{count}-extended.wav"
        write_wave(short, raw[:count * 4])
        write_wave(extended, raw[:count * 4] + bytes(48000 * 4))
        for speed in criteria["speeds"]:
            outputs, runs = {}, {}
            for mode, executable, fixture in (("control", control, short),
                                               ("candidate", player, short),
                                               ("reference", control, extended)):
                outputs[mode], runs[mode] = render(
                    executable, fixture, speed, root / f"{count}-{speed}-{mode}.pcm")
            original, candidate, reference = (outputs[k] for k in
                                               ("control", "candidate", "reference"))
            frames = len(candidate) // 4
            comparison = compare(original, candidate, reference, count, speed)
            row = {"input_frames": count, "speed": speed, "runs": runs,
                   "candidate_frames": frames, "control_frames": len(original) // 4,
                   **comparison,
                   "passed": all(r["passed"] for r in runs.values()) and comparison["passed"]}
            results.append(row)
            (root / "results.json").write_text(json.dumps(results, indent=2) + "\n")
            print(json.dumps({k: v for k, v in row.items() if k != "runs"}), flush=True)
    summary = {"cases": len(results), "passed": sum(r["passed"] for r in results),
               "control_duration_failures": sum(abs(r["control_frames"] - r["input_frames"] / r["speed"]) > 1
                                                for r in results),
               "nonzero_tail_cases": sum(r["added_tail_nonzero"] for r in results)}
    (root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return 0 if all(r["passed"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
