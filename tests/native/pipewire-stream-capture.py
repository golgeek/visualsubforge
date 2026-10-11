#!/usr/bin/env python3
"""Capture one channel of a test player through an explicit PipeWire port link.

PID SECONDS OUTPUT.f32 matches the existing audio observer's helper protocol.
VSF_PW_CAPTURE must name the compiled C helper. No default capture device or
whole-sink monitor is used. Graph timestamps are not acoustic presentation times.
"""
import argparse
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import time


def objects():
    return json.loads(subprocess.check_output(["pw-dump"], timeout=3))


def node_for_pid(snapshot, pid, media_class):
    clients = {str(o["id"]) for o in snapshot if o.get("type") == "PipeWire:Interface:Client"
               and str(o.get("info", {}).get("props", {}).get("application.process.id")) == str(pid)}
    matches = [o for o in snapshot if o.get("type") == "PipeWire:Interface:Node"
               and (str(o.get("info", {}).get("props", {}).get("application.process.id")) == str(pid)
                    or str(o.get("info", {}).get("props", {}).get("client.id")) in clients)
               and o.get("info", {}).get("props", {}).get("media.class") == media_class]
    if len(matches) != 1:
        raise ValueError(f"expected one {media_class} node for PID {pid}; found {len(matches)}")
    return matches[0]


def ports(snapshot, node, direction):
    return [o for o in snapshot if o.get("type") == "PipeWire:Interface:Port"
            and str(o.get("info", {}).get("props", {}).get("node.id")) == str(node["id"])
            and o.get("info", {}).get("props", {}).get("port.direction") == direction]


def capture(pid, seconds, path):
    helper = Path(os.environ["VSF_PW_CAPTURE"]).resolve(strict=True)
    first = objects()
    player = node_for_pid(first, pid, "Stream/Output/Audio")
    output = ports(first, player, "out")
    if len(output) == 2:
        # A mono fixture can be remixed to stereo by the server adapter. Capture
        # only its explicitly identified left channel; never merge other nodes.
        output = [o for o in output if o.get("info", {}).get("props", {}).get("audio.channel") == "FL"]
    if len(output) != 1:
        raise ValueError("diagnostic requires a mono output or an identified stereo left port")
    player_serial = player["info"]["props"]["object.serial"]
    stopped = False

    def stop(signum, frame):
        nonlocal stopped
        stopped = True

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    recorder = None
    evidence = {"player_pid": pid, "player": player, "output_port": output[0]}
    with Path(str(path) + ".log").open("xb") as log, Path(str(path) + ".link.json").open("x") as evidence_file:
        try:
            recorder = subprocess.Popen([str(helper), str(math.ceil(seconds)), str(path)],
                                        stdout=log, stderr=subprocess.STDOUT)
            deadline = time.monotonic() + 5
            while not stopped:
                if recorder.poll() is not None:
                    raise RuntimeError("capture helper exited during initialization")
                snapshot = objects()
                # Recheck identity before creating a link; numeric IDs may be reused.
                current = node_for_pid(snapshot, pid, "Stream/Output/Audio")
                if current["info"]["props"]["object.serial"] != player_serial:
                    raise RuntimeError("player node was replaced")
                try:
                    receiver = node_for_pid(snapshot, recorder.pid, "Stream/Input/Audio")
                except ValueError:
                    receiver = None
                inputs = ports(snapshot, receiver, "in") if receiver else []
                if len(inputs) == 1:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("capture input port did not become available")
                time.sleep(.02)
            if stopped:
                raise RuntimeError("capture stopped before linking")
            subprocess.run(["pw-link", "--passive", str(output[0]["id"]), str(inputs[0]["id"])],
                           check=True, timeout=3, stdout=log, stderr=subprocess.STDOUT)
            evidence.update(capture_pid=recorder.pid, receiver=receiver, input_port=inputs[0])
            links = [o for o in objects() if o.get("type") == "PipeWire:Interface:Link"
                     and o.get("info", {}).get("input-node-id") == receiver["id"]]
            if len(links) != 1 or links[0]["info"]["output-node-id"] != player["id"]:
                raise RuntimeError("capture is not exclusively linked to the selected player")
            evidence["link"] = links[0]
            print("READY pipewire-explicit-stream-graph", flush=True)
            deadline = time.monotonic() + seconds + 3
            while not stopped and recorder.poll() is None and time.monotonic() < deadline:
                time.sleep(.02)
        finally:
            if recorder:
                if recorder.poll() is None:
                    recorder.terminate()
                try:
                    recorder.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    recorder.kill()
                    recorder.wait()
                    evidence["forced_cleanup"] = True
                evidence["capture_exit"] = recorder.returncode
            # Destroying the receiver removes its ports and the ephemeral link.
            evidence_file.write(json.dumps(evidence, indent=2) + "\n")
    if not recorder or recorder.returncode or evidence.get("forced_cleanup"):
        raise RuntimeError("capture did not finish successfully")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pid", type=int)
    parser.add_argument("seconds", type=float)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.pid <= 0 or not math.isfinite(args.seconds) or not 0 < args.seconds <= 60:
        parser.error("positive PID and seconds in (0, 60] required")
    try:
        capture(args.pid, args.seconds, args.output)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        print("CAPTURE FAILED:", error, flush=True)
        raise SystemExit(1)
