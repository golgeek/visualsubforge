#!/usr/bin/env python3
"""Capture one Linux test player's PulseAudio sink input, never the whole sink.

Observer protocol: PID MAX_SECONDS OUTPUT.f32; prints READY after the recorder
stream exists. SIGTERM stops recording cleanly. Metadata records receive times,
not presentation timestamps: these must not feed the macOS timing analyzer.
Requires pactl/parec and a server supporting single-stream monitoring.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import tempfile
import time


def objects(kind):
    return json.loads(subprocess.check_output(
        ["pactl", "--format=json", "list", kind], timeout=3))


def analyze_content(path):
    spec = importlib.util.spec_from_file_location(
        "audio_tone_content", Path(__file__).with_name("audio-tone-content.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.analyze_content(path, "pulse-single-stream-receive-time")


def capture(pid, duration, path):
    inputs = [s for s in objects("sink-inputs")
              if s.get("properties", {}).get("application.process.id") == str(pid)]
    if len(inputs) != 1:
        raise ValueError(f"expected one sink input for PID {pid}; found {len(inputs)}")
    stream = inputs[0]
    sinks = [s for s in objects("sinks") if s["index"] == stream["sink"]]
    if len(sinks) != 1 or not sinks[0].get("monitor_source"):
        raise ValueError("selected player's monitor source is unavailable")
    monitor = sinks[0]["monitor_source"]
    metadata = {"capture_kind": "pulse-single-stream-receive-time",
                "player_pid": pid, "sink_input": stream["index"],
                "sink": stream["sink"], "monitor_source": monitor,
                "sample_rate": 48000, "channels": 2, "format": "float32le",
                "blocks": [], "frames": 0}
    stopped = False

    def stop(signum, frame):
        nonlocal stopped
        stopped = True

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    with path.open("xb") as output, tempfile.TemporaryFile() as log:
        recorder = subprocess.Popen([
            "parec", "--raw", "--format=float32le", "--rate=48000", "--channels=2",
            "--latency-msec=20", "--client-name=VSF isolated test capture",
            "--device=" + str(monitor), "--monitor-stream=" + str(stream["index"]),
        ], stdout=subprocess.PIPE, stderr=log)
        try:
            deadline = time.monotonic() + 5
            while not stopped:
                if recorder.poll() is not None:
                    raise RuntimeError("recorder exited before becoming ready")
                outputs = [s for s in objects("source-outputs")
                           if s.get("properties", {}).get("application.process.id")
                           == str(recorder.pid)]
                if len(outputs) == 1:
                    metadata["source_output"] = outputs[0]["index"]
                    break
                if time.monotonic() > deadline:
                    raise RuntimeError("recorder source output did not become ready")
                time.sleep(.02)
            if stopped:
                raise RuntimeError("capture stopped during startup")
            print("READY pulse-single-stream", flush=True)
            deadline = time.monotonic() + duration
            total = 0
            with selectors.DefaultSelector() as ready:
                ready.register(recorder.stdout, selectors.EVENT_READ)
                while not stopped and time.monotonic() < deadline:
                    if not ready.select(timeout=.1):
                        if recorder.poll() is not None:
                            raise RuntimeError("recorder stopped unexpectedly")
                        continue
                    data = os.read(recorder.stdout.fileno(), 65536)
                    received = time.monotonic()
                    if not data:
                        raise RuntimeError("recorder closed its PCM stream")
                    output.write(data)
                    metadata["blocks"].append({"byte_offset": total, "bytes": len(data),
                                               "received_monotonic_seconds": received})
                    total += len(data)
                    if total > (duration + 5) * 48000 * 8:
                        raise RuntimeError("capture exceeded its bounded PCM size")
            if total == 0 or total % 8:
                raise RuntimeError("capture is empty or ends in a partial stereo frame")
            metadata["frames"] = total // 8
        finally:
            if recorder.poll() is None:
                recorder.terminate()
            try:
                recorder.wait(timeout=3)
            except subprocess.TimeoutExpired:
                recorder.kill()
                recorder.wait()
            log.seek(0)
            metadata["recorder_exit"] = recorder.returncode
            metadata["recorder_log"] = log.read().decode(errors="replace")
            recorder.stdout.close()
            Path(str(path) + ".json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int)
    parser.add_argument("seconds", type=float)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.pid <= 0 or not 0 < args.seconds <= 60:
        parser.error("positive PID and capture duration in (0, 60] required")
    try:
        capture(args.pid, args.seconds, args.output)
    except (ValueError, RuntimeError, OSError, subprocess.SubprocessError) as error:
        print("CAPTURE FAILED:", error, flush=True)
        raise SystemExit(1)
