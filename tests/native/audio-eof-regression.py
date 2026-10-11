#!/usr/bin/env python3
"""Exercise keep-open EOF through events, without periodic property polling.

Checks completion and transport state, not acoustic presentation. A Python
watchdog owns the process lifetime if the in-player watchdog cannot run.
"""
import argparse
from array import array
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import wave


def check(trace, backend, mode, action_seconds, expected_source=None):
    failures = []
    if trace.get("error"):
        failures.append(trace["error"])
    if trace.get("backend") != backend:
        failures.append("Requested audio backend was not used")
    events = trace.get("events", [])
    completed = [e for e in events if e["kind"] == "completed"]
    actions = [e for e in events if e["kind"] == "action"]
    resumes = [e for e in events if e["kind"] == "resume"]
    if len(completed) != 1:
        failures.append("Expected exactly one final paused EOF")
    if mode != "ordinary" and len(actions) != 1:
        failures.append("Scheduled transition did not occur exactly once")
    if mode in ("pause", "pause-seek", "pause-reload"):
        if len(resumes) != 1 or len(actions) != 1:
            failures.append("Pause/resume incomplete")
        elif mode == "pause" and (actions[0].get("position") is None or resumes[0].get("position") is None
              or abs(actions[0]["position"] - resumes[0]["position"]) > .05):
            # Separate client queries bracket the pause command; allow dispatch
            # latency rather than treating them as simultaneous sample reads.
            failures.append("Clock missing or moved materially while paused")
    if mode in ("pause-seek", "pause-reload"):
        resets = [e for e in events if e["kind"] == "paused-reset"]
        expected_position = .5 if mode == "pause-seek" else 0
        expected_duration = 1 if mode == "pause-seek" else 2
        if len(resets) != 1 or len(resumes) != 1:
            failures.append("Paused reset did not complete exactly once")
        else:
            resumed = resumes[0]
            if resumed.get("paused") is not True:
                failures.append("Reset did not preserve paused state")
            if (resumed.get("transport_position") is None or
                    abs(resumed["transport_position"] - expected_position) > .05):
                failures.append("Paused reset retained the old source position")
            if (resumed.get("duration") is None or
                    abs(resumed["duration"] - expected_duration) > .01):
                failures.append("Paused reset retained the old source duration")
            # mpv normalizes Windows separators. Verify actual file identity
            # rather than accepting/rejecting a spelling of the path in Lua.
            try:
                source_matches = (expected_source is not None and
                                  os.path.samefile(resumed["source_path"], expected_source))
            except (KeyError, OSError, TypeError, ValueError):
                source_matches = False
            if not source_matches:
                failures.append("Paused reset resumed the wrong file")
            if completed and completed[0]["seconds"] < resumed["seconds"]:
                failures.append("Stale EOF preceded reset resume")
    if completed:
        expected = {"ordinary": 1, "pause": 1.25,
                    "seek": action_seconds + .5, "reload": action_seconds + 1,
                    "pause-seek": action_seconds + .75,
                    "pause-reload": action_seconds + 2.25}[mode]
        # A generous scheduling bound detects stuck completion, not latency
        # precision. Native startup and queued output vary by backend.
        if not expected - .2 <= completed[0]["seconds"] <= expected + 1:
            failures.append("EOF completion outside transport timing bound")
    return {"passed": not failures, "failures": failures}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", required=True)
    parser.add_argument("--ao", required=True)
    parser.add_argument("--audio-channels", choices=("mono", "stereo", "auto-safe"),
                        default="stereo", help="Explicit output-layout policy for this probe")
    parser.add_argument("--action-seconds", type=float, default=.8)
    parser.add_argument("--paused-reset", action="store_true",
                        help="Also seek and replace media while the queued tail is paused")
    parser.add_argument("--output-dir", type=Path, default=Path("tests/fixtures/generated"))
    args = parser.parse_args()
    if not 0 < args.action_seconds < 1:
        parser.error("action time must be inside the one-second fixture")
    player = shutil.which(args.mpv)
    if player is None:
        parser.error("Player executable was not found: " + args.mpv)
    args.mpv = str(Path(player).resolve())
    args.output_dir.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="audio-eof-", dir=args.output_dir)).resolve()
    media = root / "tone.wav"
    samples = array("h", (round(8000 * math.sin(2 * math.pi * 440 * i / 48000))
                           for i in range(48000)))
    with wave.open(str(media), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(48000)
        output.writeframes(samples.tobytes())
    replacement = root / "replacement.wav"
    with wave.open(str(replacement), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(48000)
        output.writeframes(samples.tobytes() * 2)
    results = []
    modes = ["ordinary", "pause", "seek", "reload"]
    if args.paused_reset:
        modes += ["pause-seek", "pause-reload"]
    for mode in modes:
        trace_path = root / (mode + ".json")
        config = {"mode": mode, "output": str(trace_path), "media": str(media),
                  "replacement": str(replacement),
                  "action_seconds": args.action_seconds}
        command = [args.mpv, "--no-config", "--vo=null", "--no-audio-display",
                   "--audio-channels=" + args.audio_channels, "--keep-open=yes", "--idle=yes",
                   "--terminal=no", "--osd-level=0", "--osc=no", "--load-scripts=no",
                   "--input-terminal=no", "--ao=" + args.ao,
                   "--log-file=" + str(root / (mode + ".native.log")),
                   "--script=" + str(Path(__file__).with_name("audio-eof-observer.lua").resolve()),
                   str(media)]
        (root / (mode + ".command.json")).write_text(json.dumps(command, indent=2) + "\n")
        with (root / (mode + ".process.log")).open("w") as log:
            try:
                run = subprocess.run(command, env={**os.environ, "VSF_AUDIO_EOF_CONFIG": json.dumps(config)},
                                     stdout=log, stderr=subprocess.STDOUT, timeout=10)
                trace = json.loads(trace_path.read_text()) if trace_path.exists() else {"error": "No trace"}
                expected_source = replacement if mode == "pause-reload" else media
                result = check(trace, args.ao, mode, args.action_seconds, expected_source)
                if run.returncode != 0:
                    result["passed"] = False
                    result["failures"].append("Player exited " + str(run.returncode))
            except subprocess.TimeoutExpired:
                result = {"passed": False, "failures": ["Python watchdog expired; player killed and reaped"]}
            except (OSError, ValueError) as error:
                result = {"passed": False, "failures": [str(error)]}
        result.update(mode=mode, backend=args.ao, trace=str(trace_path))
        results.append(result)
        (root / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        print(json.dumps(result), flush=True)
    return 0 if all(r["passed"] for r in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
