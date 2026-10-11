#!/usr/bin/env python3
"""Observe mpv's short audio-only loop clock; not a pass/fail sync test.

Uses the CLI's JSON IPC with no video window. Each case has a fresh process;
audio options affect only that process, never the system device configuration.
Run on an unlocked Mac with the generated long.flac fixture. See task 005.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import selectors
import subprocess
import tempfile
import time


def observe(mpv, media, name, options, *, start=7198.93, end=7199.253,
            duration=2.5, interval=0.02, looping=True, speed=1.0,
            capture=None, capture_path=None, actions=(), properties=(), log_path=None):
    with tempfile.TemporaryDirectory(prefix="vsf-audio-loop-") as directory:
        ipc = Path(directory) / "mpv.sock"
        with tempfile.TemporaryFile(mode="w+t") as log:
            process = subprocess.Popen(
                [mpv, "--no-config", "--vo=null", "--no-audio-display",
                 "--pause", "--keep-open=yes", "--hr-seek-demuxer-offset=1",
                 "--msg-level=cplayer=v", "--ab-loop-count=1000",
                 f"--input-ipc-server={ipc}", *options, str(media)],
                stdout=log, stderr=subprocess.STDOUT,
            )
            result = {"case": name, "options": options, "media": str(media),
                      "start": start, "end": end, "looping": looping,
                      "speed": speed}
            recorder = None
            try:
                deadline = time.monotonic() + 5
                while not ipc.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("mpv did not create its IPC socket")
                    time.sleep(0.02)
                with socket.socket(socket.AF_UNIX) as connection:
                    connection.settimeout(3)
                    # Socket creation can precede listen(). Retry that startup
                    # race, but never restart an already-running mpv process.
                    while True:
                        try:
                            connection.connect(str(ipc))
                            break
                        except ConnectionRefusedError:
                            if process.poll() is not None or time.monotonic() > deadline:
                                raise
                            time.sleep(0.02)
                    with connection.makefile("rwb", buffering=0) as stream:
                        request_id = 0

                        events = []

                        def command(args, optional=False):
                            nonlocal request_id
                            request_id += 1
                            stream.write((json.dumps({"command": args,
                                          "request_id": request_id}) + "\n").encode())
                            while True:
                                line = stream.readline()
                                if not line:
                                    raise RuntimeError("mpv closed IPC")
                                reply = json.loads(line)
                                if reply.get("request_id") == request_id:
                                    if reply.get("error") != "success" and not optional:
                                        raise RuntimeError(str(reply))
                                    return reply.get("data")
                                if "event" in reply:
                                    events.append({"wall_seconds": time.monotonic(),
                                                   **reply})

                        while not command(["get_property", "duration"], optional=True):
                            if time.monotonic() > deadline:
                                raise RuntimeError("media did not become ready")
                            time.sleep(0.02)
                        while True:
                            output = command(["get_property", "current-ao"], optional=True)
                            if output:
                                result["audio_output"] = output
                                break
                            if time.monotonic() > deadline:
                                raise RuntimeError("audio output did not become ready")
                            time.sleep(0.02)
                        if capture:
                            if capture_path is None:
                                raise ValueError("capture_path is required for audio capture")
                            recorder = subprocess.Popen(
                                [str(capture), str(process.pid), str(duration + 2), str(capture_path)],
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            )
                            with selectors.DefaultSelector() as ready:
                                ready.register(recorder.stdout, selectors.EVENT_READ)
                                capture_deadline = time.monotonic() + 10
                                phases = []
                                buffered = b""
                                message = ""
                                while not message.startswith("READY "):
                                    remaining = capture_deadline - time.monotonic()
                                    if remaining <= 0 or not ready.select(timeout=remaining):
                                        raise RuntimeError("audio capture did not become ready; check OS permission")
                                    chunk = os.read(recorder.stdout.fileno(), 4096)
                                    if not chunk:
                                        raise RuntimeError("audio capture exited before becoming ready")
                                    buffered += chunk
                                    while b"\n" in buffered:
                                        line, buffered = buffered.split(b"\n", 1)
                                        message = line.decode(errors="replace")
                                        if message.startswith("READY "):
                                            break
                                        phases.append(message)
                                        result["capture_phases"] = phases
                            result["capture_ready"] = message
                            if not message.startswith("READY "):
                                raise RuntimeError(f"audio capture startup failed: {message}")
                        setup = [["set_property", "speed", speed]]
                        if looping:
                            setup.extend([
                                ["set_property", "ab-loop-a", start],
                                ["set_property", "ab-loop-b", end],
                            ])
                        setup.extend([
                            ["seek", start, "absolute+exact"],
                            ["set_property", "pause", False],
                        ])
                        for args in setup:
                            command(args)
                        positions = []
                        samples = []
                        began = time.monotonic()
                        result["sample_origin_monotonic_seconds"] = began
                        result["transitions"] = []
                        pending_actions = iter(actions)
                        next_action = next(pending_actions, None)
                        phase = "initial"
                        while time.monotonic() - began < duration:
                            time.sleep(interval)
                            elapsed = time.monotonic() - began
                            while next_action and elapsed >= next_action[0]:
                                scheduled, phase, commands = next_action
                                transition = {"phase": phase, "scheduled_seconds": scheduled,
                                              "before_seconds": time.monotonic() - began,
                                              "commands": commands}
                                for args in commands:
                                    if args[0] in ("diagnostic-clear-loop-at-phase",
                                                   "diagnostic-clear-loop-at-audio-pts",
                                                   "diagnostic-refresh-at-audio-pts"):
                                        # Candidate transport policy, deliberately
                                        # restricted to a known steady test range.
                                        # Retain its raw input and resolved seek;
                                        # this is not an application clock fix.
                                        raw = command(["get_property", "audio-pts"])
                                        target = raw
                                        if args[0] == "diagnostic-clear-loop-at-phase":
                                            loop_start, loop_end = args[1:]
                                            target = loop_start + (raw - loop_start) % (loop_end - loop_start)
                                        transition["raw_audio_pts"] = raw
                                        transition["resolved_seek_seconds"] = target
                                        if args[0] != "diagnostic-refresh-at-audio-pts":
                                            command(["set_property", "ab-loop-a", "no"])
                                            command(["set_property", "ab-loop-b", "no"])
                                        command(["seek", target, "absolute+exact"])
                                    elif args[0] == "diagnostic-loadfile-and-wait":
                                        first_event = len(events)
                                        command(["loadfile", args[1], "replace"])
                                        ready_by = time.monotonic() + 5
                                        while True:
                                            path = command(["get_property", "path"], optional=True)
                                            loaded = any(e["event"] == "file-loaded" for e in events[first_event:])
                                            if loaded and path == args[1]:
                                                break
                                            if time.monotonic() >= ready_by:
                                                raise RuntimeError("replacement file did not become ready")
                                            time.sleep(0.02)
                                    else:
                                        command(args)
                                transition["after_seconds"] = time.monotonic() - began
                                result["transitions"].append(transition)
                                next_action = next(pending_actions, None)
                            sample = {"elapsed_seconds": time.monotonic() - began, "phase": phase}
                            for prop in ("time-pos", "audio-pts", "remaining-ab-loops",
                                         "seeking", "eof-reached", "pause", *properties):
                                sample[prop] = command(["get_property", prop], optional=True)
                            samples.append(sample)
                            if sample["time-pos"] is not None:
                                positions.append(sample["time-pos"])
                        result["positions_seconds"] = positions
                        result["samples"] = samples
                        result["events"] = [
                            {**event, "wall_seconds": event["wall_seconds"] - began}
                            for event in events
                        ]
            except (OSError, RuntimeError, ValueError) as error:
                result["error"] = str(error)
            finally:
                if recorder:
                    recorder.terminate()
                    try:
                        recorder_output, _ = recorder.communicate(timeout=5)
                    except subprocess.TimeoutExpired:
                        recorder.kill()
                        recorder_output, _ = recorder.communicate()
                    result["capture_exit"] = recorder.returncode
                    result["capture_log"] = recorder_output.decode(errors="replace")
                    result["capture_path"] = str(capture_path)
                    if recorder.returncode != 0 and "error" not in result:
                        result["error"] = "audio capture did not finish successfully"
                if process.poll() is None:
                    process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                log.seek(0)
                native_log = log.read()
                result["log_tail"] = native_log[-12000:]
                if log_path:
                    with Path(log_path).open("x") as output:
                        output.write(native_log)
                    result["native_log_path"] = str(log_path)
            return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", default="mpv")
    parser.add_argument("--media", type=Path,
                        default=Path("tests/fixtures/generated/long.flac"))
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--matrix", action="store_true",
                      help="Compare ordinary seek, loop length, speed and output buffers")
    mode.add_argument("--drain-matrix", action="store_true",
                      help="Compare gapless=no across default, CoreAudio and null output")
    mode.add_argument("--capture-matrix", action="store_true",
                      help="Compare steady loop output at several lengths/speeds; requires --capture")
    mode.add_argument("--transitions", action="store_true",
                      help="Observe pause, outside seeks, range changes, clear-loop and EOF")
    mode.add_argument("--flush-transitions", action="store_true",
                      help="Test an explicit seek to the steady audio phase when clearing a loop")
    parser.add_argument("--capture", type=Path, help="Path to the macOS process-audio-capture helper")
    parser.add_argument("--capture-dir", type=Path, default=Path("tests/fixtures/generated"))
    args = parser.parse_args()
    media = args.media.resolve(strict=True)
    capture_directory = None
    if args.capture:
        args.capture = args.capture.resolve(strict=True)
        args.capture_dir.mkdir(parents=True, exist_ok=True)
        capture_directory = Path(tempfile.mkdtemp(prefix="audio-capture-", dir=args.capture_dir)).resolve()
    elif args.capture_matrix:
        parser.error("--capture-matrix requires --capture")
    cases = [(name, options, {}) for name, options in [
        ("default", []), ("float", ["--audio-format=float"]),
        ("stereo", ["--audio-channels=stereo"]),
        ("48k", ["--audio-samplerate=48000"]),
        ("float-stereo", ["--audio-format=float", "--audio-channels=stereo"]),
    ]]
    if args.matrix:
        cases = []
        for name, options in [
            ("default", []), ("stereo", ["--audio-channels=stereo"]),
            ("null", ["--ao=null"]),
            ("stereo-buffer20ms", ["--audio-channels=stereo", "--audio-buffer=0.02"]),
        ]:
            for mode, kwargs in [
                ("seek", {"start": 100.0, "looping": False}),
                ("short", {"start": 100.0, "end": 100.323}),
                ("long", {"start": 100.0, "end": 102.0, "duration": 5.0}),
                ("half-speed", {"start": 100.0, "end": 100.323, "speed": 0.5}),
            ]:
                cases.append((f"{name}-{mode}", options, kwargs))
    elif args.drain_matrix:
        cases = [(name, [*options, "--gapless-audio=no"],
                  {"start": 100.0, "end": 100.323, "duration": 8.0})
                 for name, options in [
                     ("default-drain", []),
                     ("stereo-drain", ["--audio-channels=stereo"]),
                     ("null-drain", ["--ao=null"]),
                 ]]
    elif args.transitions or args.flush_transitions:
        transitions = [
            (3, "paused", [["set_property", "pause", True]]),
            (4, "resumed", [["set_property", "pause", False]]),
            (7, "seek-before-a", [["seek", 7198.530, "absolute+exact"]]),
            (10, "seek-after-b", [["seek", 7199.500, "absolute+exact"]]),
            (12, "seek-back-a", [["seek", 7198.930, "absolute+exact"], ["set_property", "pause", False]]),
            (15, "changed-range", [["set_property", "ab-loop-b", 7199.450],
                                    ["seek", 7198.930, "absolute+exact"]]),
            (18, "cleared-loop", [["set_property", "ab-loop-a", "no"],
                                   ["set_property", "ab-loop-b", "no"]]),
        ]
        if args.flush_transitions:
            transitions[-1] = (18, "cleared-loop-with-seek", [
                ["diagnostic-clear-loop-at-phase", 7198.930, 7199.450]])
        cases = [(name, options, {"duration": 23.0, "actions": transitions})
                 for name, options in [
                     ("default-flush" if args.flush_transitions else "default-transitions", []),
                     ("stereo-flush" if args.flush_transitions else "stereo-transitions", ["--audio-channels=stereo"])]]
    elif args.capture_matrix:
        cases = [(f"{output}-{case}", options, kwargs)
                 for output, options in [("default", []), ("stereo", ["--audio-channels=stereo"])]
                 for case, kwargs in [
                     ("short", {"duration": 6.0}),
                     ("long", {"start": 7197.5, "end": 7199.5, "duration": 10.0}),
                     ("half-speed", {"speed": 0.5, "duration": 8.0}),
                     ("fast", {"speed": 1.5, "duration": 6.0}),
                 ]]
    failed = False
    for name, options, kwargs in cases:
        if capture_directory:
            kwargs = {**kwargs, "capture": args.capture,
                      "capture_path": capture_directory / f"{name}.f32"}
        result = observe(args.mpv, media, name, options, **kwargs)
        print(json.dumps(result), flush=True)
        failed |= "error" in result
    # A successful observation is still not a timing PASS. Nonzero means the
    # harness could not gather one or more cases and their error must be read.
    raise SystemExit(1 if failed else 0)
