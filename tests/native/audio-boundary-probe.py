#!/usr/bin/env python3
"""Check decoded loop boundaries through mpv's PCM sink, not the speakers.

An exact sample comparison isolates seek/trim correctness from audio-device
latency and clock reporting. The candidate contract rounds each endpoint to
the nearest source sample (half-sample ties forward), using integer arithmetic
for the oracle. This is deliberately not an audible timing PASS.
Only synthetic input is used; all generated media stays in a temporary folder.
"""
import argparse
from array import array
from contextlib import nullcontext
import json
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import wave


def check(mpv, rate, gapless, output_dir=None):
    # A deterministic quiet sequence distinguishes both boundaries from nearby
    # samples; silence or a single pulse would miss some erroneous extensions.
    rng = random.Random(5)
    samples = array("h", (rng.randrange(-2000, 2001) for _ in range(rate * 3)))
    if sys.byteorder != "little":
        samples.byteswap()
    start_ms, end_ms, repeats = 1013, 1336, 4
    start_sample = (start_ms * rate + 500) // 1000
    end_sample = (end_ms * rate + 500) // 1000
    expected = samples.tobytes()[start_sample * 2:end_sample * 2] * repeats
    folder = (nullcontext(tempfile.mkdtemp(prefix="audio-boundary-", dir=output_dir))
              if output_dir else tempfile.TemporaryDirectory(prefix="vsf-audio-boundary-"))
    with folder as directory:
        source = Path(directory) / "source.wav"
        output = Path(directory) / "output.pcm"
        with wave.open(str(source), "wb") as wav:
            wav.setparams((1, 2, rate, 0, "NONE", "not compressed"))
            wav.writeframes(samples.tobytes())
        command = [mpv, "--no-config", "--vo=null", "--no-audio-display",
                   "--ao=pcm", "--ao-pcm-waveheader=no", f"--ao-pcm-file={output}",
                   "--audio-format=s16", "--audio-channels=mono", "--volume=100",
                   f"--start={start_ms / 1000}", f"--end={end_ms / 1000}",
                   f"--ab-loop-a={start_ms / 1000}", f"--ab-loop-b={end_ms / 1000}",
                   f"--ab-loop-count={repeats - 1}", f"--gapless-audio={gapless}",
                   str(source)]
        if output_dir:
            command.insert(1, "--msg-level=all=v,cplayer=debug")
        run = subprocess.run(command, capture_output=True, timeout=15)
        actual = output.read_bytes() if output.exists() else b""
        result = {"sample_rate": rate, "gapless": gapless, "repeats": repeats,
                  "rounding": "nearest-half-forward",
                  "start_sample": start_sample, "end_sample": end_sample,
                  "expected_samples": len(expected) // 2,
                  "actual_samples": len(actual) // 2, "process_exit": run.returncode,
                  "exact_pcm_match": actual == expected and run.returncode == 0}
        if output_dir:
            log = Path(directory) / "native.log"
            log.write_bytes(run.stdout + run.stderr)
            result["evidence_directory"] = str(Path(directory).resolve())
            (Path(directory) / "command.json").write_text(json.dumps(command) + "\n")
        if not result["exact_pcm_match"]:
            # Diagnose sample rounding separately from content corruption. Keep
            # the exact comparison failing; do not silently relax its contract.
            source_bytes = samples.tobytes()
            matches = []
            for start_delta in range(-2, 3):
                for end_delta in range(-2, 3):
                    segment = source_bytes[(start_sample + start_delta) * 2:
                                           (end_sample + end_delta) * 2]
                    if segment * repeats == actual:
                        matches.append({"start_delta_samples": start_delta,
                                        "end_delta_samples": end_delta})
            result["nearby_exact_segments"] = matches
            result["expected_first_bytes"] = expected[:16].hex()
            result["actual_first_bytes"] = actual[:16].hex()
            result["log_tail"] = (run.stdout + run.stderr).decode(errors="replace")[-2000:]
        return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", default="mpv")
    parser.add_argument("--output-dir", type=Path,
                        help="Retain fixtures, PCM and verbose logs in this directory")
    args = parser.parse_args()
    if args.output_dir:
        args.output_dir.mkdir(parents=True, exist_ok=True)
    passed = True
    for rate in (8000, 44100, 48000, 96000):
        for gapless in ("weak", "no"):
            result = check(args.mpv, rate, gapless, args.output_dir)
            print(json.dumps(result), flush=True)
            passed &= result["exact_pcm_match"]
    raise SystemExit(0 if passed else 1)
