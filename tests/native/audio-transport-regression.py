#!/usr/bin/env python3
"""Check queued-clock transport through an actual selected mpv audio backend.

Uses synthetic audio with a POSIX IPC or portable in-player Lua observer. This checks properties
and command responsiveness, not captured output or acoustic synchronization.
Raw traces are retained, including startup and transition observations.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check(trace):
    failures = []
    if "error" in trace:
        return {"passed": False, "failures": [trace["error"]]}
    samples = trace["samples"]
    transitions = trace["transitions"]
    phase_start = {"initial": 0, **{t["phase"]: t["after_seconds"] for t in transitions}}
    settled = [s for s in samples
               if s["elapsed_seconds"] > phase_start[s["phase"]] + .5]
    phases = {name: [s for s in settled if s["phase"] == name]
              for name in ["initial", "paused", "resumed", "cleared"]}
    tolerance = 1 / 48000
    wrap_count = 0
    for name in ["initial", "resumed"]:
        group = phases[name]
        positions = [s["audio-pts"] for s in group if s["audio-pts"] is not None]
        if len(positions) != len(group) or len(positions) < 10:
            failures.append(name + ": missing live clock observations")
        if any(p < trace["start"] - tolerance or p > trace["end"] + tolerance for p in positions):
            failures.append(name + ": clock outside loop bounds")
        wraps = sum(b < a - .1 for a, b in zip(positions, positions[1:]))
        wrap_count += wraps
        if wraps < 2 or len(set(positions)) < 10:
            failures.append(name + ": insufficient clock progression/wraps")
    paused = phases["paused"]
    positions = [s["audio-pts"] for s in paused if s["audio-pts"] is not None]
    if (not positions or len(positions) != len(paused)
            or not all(s["pause"] for s in paused)
            or max(positions) - min(positions) > tolerance):
        failures.append("paused: clock unavailable, moving or not paused")
    cleared = phases["cleared"]
    live = [s for s in cleared if not s["eof-reached"]]
    backsteps = [a["audio-pts"] - b["audio-pts"] for a, b in zip(live, live[1:])
                 if a["audio-pts"] is not None and b["audio-pts"] is not None]
    if not live or any(s["audio-pts"] is None for s in live):
        failures.append("cleared: missing live clock")
    elif any(b["audio-pts"] < a["audio-pts"] - tolerance for a, b in zip(live, live[1:])):
        failures.append("cleared: clock moved backwards")
    if not cleared or not cleared[-1]["eof-reached"]:
        failures.append("cleared: did not reach EOF")
    durations = {t["phase"]: t["after_seconds"] - t["before_seconds"] for t in transitions}
    if any(seconds > .250 for seconds in durations.values()):
        failures.append("transport transaction exceeded 250 ms")
    return {"passed": not failures, "failures": failures, "wraps": wrap_count,
            "transaction_seconds": durations, "settle_seconds": .5,
            "max_backward_step_seconds": max([0, *backsteps]),
            "clock_tolerance_seconds": tolerance}


def check_ordinary(trace):
    if "error" in trace:
        return {"passed": False, "failures": [trace["error"]]}
    starts = {"initial": 0, **{t["phase"]: t["after_seconds"] for t in trace["transitions"]}}
    samples = [s for s in trace["samples"] if s["elapsed_seconds"] > starts[s["phase"]] + .5]
    live = [s for s in samples if not s["eof-reached"]]
    positions = [s["audio-pts"] for s in live if s["audio-pts"] is not None]
    backward = max([0, *[a["audio-pts"] - b["audio-pts"] for a, b in zip(live, live[1:])
                         if a["phase"] == b["phase"] and a["audio-pts"] is not None
                         and b["audio-pts"] is not None]])
    failures = []
    if len(positions) != len(live) or len(positions) < 10:
        failures.append("ordinary: missing live clock observations")
    if backward > 1 / 48000:
        failures.append("ordinary: clock moved backwards")
    if not samples or not samples[-1]["eof-reached"]:
        failures.append("ordinary: did not reach EOF")
    return {"passed": not failures, "failures": failures,
            "max_backward_step_seconds": backward, "settle_seconds": .5,
            "clock_tolerance_seconds": 1 / 48000}


def observe_lua(mpv, media, backend, folder, policy, capture_wasapi=None, mpv_options=()):
    """Use the engine's Lua client API; this does not test application IPC."""
    raw_path = folder / "lua-trace.json"
    log_path = folder / "native.log"
    config = {"start": 1.013, "end": 1.336, "duration": 10,
              "looping": True, "actions": [], **policy,
              "output": str(raw_path), "media": str(media), "backend": backend}
    capture_path = folder / "output.f32"
    if capture_wasapi:
        config["capture_ready"] = str(folder / "capture.ready")
        config["capture_clock"] = str(capture_wasapi)
    environment = {**os.environ, "VSF_AUDIO_TRANSPORT_CONFIG": json.dumps(config)}
    command = [mpv, "--no-config", "--vo=null", "--no-audio-display", "--pause",
               "--keep-open=yes", "--hr-seek-demuxer-offset=1", "--msg-level=cplayer=v",
               "--ab-loop-count=1000", "--ao=" + backend,
               "--script=" + str(Path(__file__).with_name("audio-transport-observer.lua").resolve()),
               *mpv_options, str(media)]
    failure = None
    exit_code = None
    capture_exit = None
    process = recorder = None
    with log_path.open("xb") as log, (folder / "capture.log").open("xb") as capture_log:
        try:
            process = subprocess.Popen(command, env=environment, stdout=log,
                                       stderr=subprocess.STDOUT)
            if capture_wasapi:
                recorder = subprocess.Popen(
                    [str(capture_wasapi), str(process.pid), str(config["duration"] + 3),
                     str(capture_path), config["capture_ready"]],
                    stdout=capture_log, stderr=subprocess.STDOUT)
            exit_code = process.wait(timeout=config["duration"] + 15)
            if exit_code:
                failure = f"mpv exited {exit_code}"
            if recorder:
                capture_exit = recorder.wait(timeout=10)
                if capture_exit:
                    failure = (failure + "; " if failure else "") + f"capture exited {capture_exit}"
        except (OSError, subprocess.TimeoutExpired) as error:
            failure = str(error)
        finally:
            for child in (process, recorder):
                if child and child.poll() is None:
                    child.kill()
                    child.wait()
    try:
        trace = json.loads(raw_path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        trace = {"error": "Lua trace unavailable: " + str(error)}
    # Lua omits nil-valued object properties. Preserve unavailability explicitly;
    # it is never a zero position or a false flag.
    for sample in trace.get("samples", []):
        for prop in ("time-pos", "audio-pts", "remaining-ab-loops", "seeking",
                     "eof-reached", "pause", "current-ao", "vsf-audio-clock-version"):
            sample.setdefault(prop, None)
    if failure:
        trace["error"] = "; ".join(filter(None, [trace.get("error"), failure]))
    trace.update(observer="lua-client-api", timing_basis="mp.get_time",
                 native_log_path=str(log_path), player_exit=exit_code,
                 extra_mpv_options=list(mpv_options))
    if capture_wasapi:
        trace.update(capture_path=str(capture_path), capture_exit=capture_exit,
                     capture_kind="windows-process-loopback")
    return trace


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", required=True)
    parser.add_argument("--ao", required=True, help="One explicit mpv audio backend")
    parser.add_argument("--observer", choices=("ipc", "lua"), default="ipc",
                        help="POSIX socket or portable Lua client API (not application IPC)")
    parser.add_argument("--mpv-option", action="append", default=[],
                        help="Extra diagnostic player option; use --mpv-option=--name=value")
    parser.add_argument("--capture-pulse", action="store_true",
                        help="Capture only this player's PulseAudio stream and check tone content")
    parser.add_argument("--capture-wasapi", type=Path,
                        help="Windows process-loopback helper executable; requires Lua/WASAPI")
    parser.add_argument("--capture-pipewire", type=Path,
                        help="Compiled PipeWire capture helper; records graph data for separate analysis")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--ordinary", action="store_true",
                        help="Compare ordinary playback without loops or clear commands")
    mode.add_argument("--ordinary-refresh", action="store_true",
                      help="Compare pause/read-clock/exact-seek/resume without loops")
    parser.add_argument("--output-dir", type=Path, default=Path("tests/fixtures/generated"))
    args = parser.parse_args()
    if args.capture_pulse and args.ao != "pulse":
        parser.error("--capture-pulse requires --ao pulse")
    if args.capture_pulse and args.observer != "ipc":
        parser.error("--capture-pulse requires --observer ipc")
    if args.capture_wasapi and (args.observer != "lua" or args.ao != "wasapi"):
        parser.error("--capture-wasapi requires --observer lua --ao wasapi")
    if args.capture_pipewire and (args.observer != "ipc" or args.ao != "pipewire"):
        parser.error("--capture-pipewire requires --observer ipc --ao pipewire")
    if args.capture_pipewire:
        if not args.capture_pipewire.is_file() or not os.access(args.capture_pipewire, os.X_OK):
            parser.error("--capture-pipewire must name an executable capture helper")
        os.environ["VSF_PW_CAPTURE"] = str(args.capture_pipewire.resolve())
    fixture = load("continuation_fixture", "audio-continuation-probe.py")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix="audio-transport-", dir=args.output_dir)).resolve()
    media = folder / "tones.wav"
    fixture.make_fixture(media)
    actions = [
        (2, "paused", [["set_property", "pause", True]]),
        (3, "resumed", [["set_property", "pause", False]]),
        (5, "cleared", [["set_property", "pause", True],
                         ["diagnostic-clear-loop-at-audio-pts"],
                         ["set_property", "pause", False]]),
    ]
    ordinary = args.ordinary or args.ordinary_refresh
    policy = ({"start": 0, "looping": False, "duration": 5} if ordinary else
              {"start": 1.013, "end": 1.336, "duration": 10, "actions": actions})
    if args.ordinary_refresh:
        policy["actions"] = [(1, "refreshed", [["set_property", "pause", True],
                            ["diagnostic-refresh-at-audio-pts"],
                            ["set_property", "pause", False]])]
    capture_path = folder / "output.f32"
    capture_options = ({"capture": Path(__file__).with_name("pulse-stream-capture.py").resolve(),
                        "capture_path": capture_path} if args.capture_pulse else {})
    if args.capture_pipewire:
        capture_options = {"capture": Path(__file__).with_name("pipewire-stream-capture.py").resolve(),
                           "capture_path": capture_path}
    if args.observer == "lua":
        trace = observe_lua(args.mpv, media, args.ao, folder, policy, args.capture_wasapi,
                            args.mpv_option)
    else:
        observer = load("loop_observer", "audio-loop-probe.py")
        trace = observer.observe(args.mpv, media, args.ao, ["--ao=" + args.ao, *args.mpv_option],
                                 **policy, **capture_options,
                                 properties=("current-ao", "vsf-audio-clock-version"),
                                 log_path=folder / "native.log")
        trace.update(observer="posix-json-ipc", timing_basis="time.monotonic")
    path = folder / "trace.json"
    path.write_text(json.dumps(trace, indent=2) + "\n")
    result = {"backend": args.ao, "observer": trace["observer"], "ordinary": ordinary,
              "refresh": args.ordinary_refresh, "trace": str(path),
              **(check_ordinary(trace) if ordinary else check(trace))}
    if args.capture_pipewire:
        result["capture_kind"] = "pipewire-explicit-stream-graph"
        result["capture_analysis"] = "Run pipewire-graph-analysis.py; clock assertions do not validate presentation"
    if args.capture_pulse or args.capture_wasapi:
        try:
            capture_module = load("audio_tone_content", "audio-tone-content.py")
            capture_kind = ("windows-process-loopback" if args.capture_wasapi
                            else "pulse-single-stream-receive-time")
            result["captured_content"] = capture_module.analyze_content(capture_path, capture_kind)
            if not result["captured_content"]["passed"]:
                result["failures"].append("captured tone content check failed")
                result["passed"] = False
        except (OSError, ValueError, KeyError) as error:
            result["failures"].append("capture analysis: " + str(error))
            result["passed"] = False
    (folder / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result), flush=True)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
