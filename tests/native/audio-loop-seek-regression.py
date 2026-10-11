#!/usr/bin/env python3
"""Ensure exact and approximate seeks can leave and re-enter an A-B loop.

Uses synthetic audio and an explicit output (null by default). This validates
transport state, not captured sound; the Lua observer also runs on Windows.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import tempfile


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check(trace, target, exact):
    if "error" in trace:
        return [trace["error"]]
    failures = []
    starts = {"initial": 0, **{t["phase"]: t["after_seconds"] for t in trace["transitions"]}}
    phases = {name: [s for s in trace["samples"] if s["phase"] == name
                    and s["elapsed_seconds"] > starts[name] + .5]
              for name in starts}
    paused = phases["sought-paused"]
    if not paused or any(not s["pause"] or s["time-pos"] is None for s in paused):
        failures.append("missing paused seek position")
    elif exact and any(abs(s["time-pos"] - target) > 1 / 48000 for s in paused):
        failures.append("exact seek did not reach its target")
    if target > trace["end"]:
        outside = phases["outside-playing"]
        if not outside or any(s["time-pos"] is not None and s["time-pos"] < trace["end"]
                              for s in outside):
            failures.append("seek past B unexpectedly returned to the loop")
        if not any(s["eof-reached"] for s in outside):
            failures.append("outside playback did not reach EOF")
    returned = phases["returned"]
    positions = [s["audio-pts"] for s in returned if s["audio-pts"] is not None]
    if len(positions) != len(returned) or len(positions) < 10:
        failures.append("missing clock after returning to A")
    if any(p < trace["start"] - 1 / 48000 or p > trace["end"] + 1 / 48000 for p in positions):
        failures.append("return seek did not restore loop bounds")
    if sum(b < a - .1 for a, b in zip(positions, positions[1:])) < 3:
        failures.append("return seek did not restore repeated playback")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", required=True)
    parser.add_argument("--ao", default="null")
    parser.add_argument("--observer", choices=("ipc", "lua"), default="ipc")
    parser.add_argument("--output-dir", type=Path, default=Path("tests/fixtures/generated"))
    args = parser.parse_args()
    observer = load("loop_observer", "audio-loop-probe.py") if args.observer == "ipc" else None
    portable = load("transport_observer", "audio-transport-regression.py") if args.observer == "lua" else None
    fixture = load("continuation_fixture", "audio-continuation-probe.py")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix="audio-loop-seeks-", dir=args.output_dir)).resolve()
    media = folder / "tones.wav"
    fixture.make_fixture(media)
    failed = False
    for name, target, mode in [("past-b-exact", 2.5, "absolute+exact"),
                               ("past-b-keyframes", 2.5, "absolute+keyframes"),
                               ("before-a-exact", .5, "absolute+exact")]:
        policy = {"start": 1.013, "end": 1.336, "duration": 8,
            "actions": [
                (1.5, "sought-paused", [["set_property", "pause", True], ["seek", target, mode]]),
                (2.5, "outside-playing", [["set_property", "pause", False]]),
                (5, "returned", [["seek", 1.013, "absolute+exact"], ["set_property", "pause", False]]),
            ]}
        if portable:
            case_folder = folder / name
            case_folder.mkdir()
            trace = portable.observe_lua(args.mpv, media, args.ao, case_folder, policy)
        else:
            trace = observer.observe(args.mpv, media, name, ["--ao=" + args.ao],
                                     **policy, log_path=folder / (name + "-native.log"))
        path = folder / (name + ".json")
        path.write_text(json.dumps(trace, indent=2) + "\n")
        failures = check(trace, target, mode == "absolute+exact")
        print(json.dumps({"case": name, "backend": args.ao, "observer": args.observer,
                          "trace": str(path), "passed": not failures,
                          "failures": failures}), flush=True)
        failed |= bool(failures)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
