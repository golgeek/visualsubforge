#!/usr/bin/env python3
"""Opt-in software-composition evidence for VSF's production ASS preview bytes.

Run the ignored Rust native_demux_preserves_milliseconds test first. This uses
real mpv/libass but does not establish desktop/GPU/audio/platform acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def run(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, timeout=30, **kwargs)


def region(rgb, predicate, top, bottom):
    points = []
    for y in range(top, bottom):
        for x in range(768):
            offset = (y * 768 + x) * 3
            if predicate(*rgb[offset:offset + 3]):
                points.append((x, y))
    return {"pixels": len(points),
            "x": sum(x for x, _ in points) / len(points) if points else None,
            "y": sum(y for _, y in points) / len(points) if points else None}


def replacements(mpv, url):
    """Exercise the same public track commands against a real owned process.

    Unix socket diagnostic (macOS/Linux); no GUI, files or other player sessions.
    The process and socket close even when an assertion/command fails.
    """
    with tempfile.TemporaryDirectory(prefix="vsf-ass-") as directory:
        path = str(Path(directory) / "mpv.sock")
        child = subprocess.Popen([str(mpv), "--no-config", "--idle=yes", "--pause",
                                  "--load-scripts=no", "--osc=no", "--ytdl=no",
                                  "--vo=null", "--ao=null", "--input-ipc-server=" + path],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            with socket.socket(socket.AF_UNIX) as connection:
                deadline = time.monotonic() + 10
                while not Path(path).exists():
                    if child.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Native player did not create its diagnostic socket")
                    time.sleep(0.02)
                connection.connect(path)
                connection.settimeout(5)
                with connection.makefile("rwb", buffering=0) as stream:
                    request_id = 0

                    def command(*args):
                        nonlocal request_id
                        request_id += 1
                        stream.write((json.dumps({"command": args, "request_id": request_id}) + "\n").encode())
                        while True:
                            line = stream.readline()
                            if not line:
                                raise RuntimeError("Native diagnostic connection closed")
                            reply = json.loads(line)
                            if reply.get("request_id") == request_id:
                                assert reply["error"] == "success", (args[0], reply)
                                return reply.get("data")

                    command("loadfile", "av://lavfi:color=c=black:s=768x576:d=6")
                    while command("get_property", "track-list/count") != 1:
                        if time.monotonic() > deadline:
                            raise RuntimeError("Native media did not become ready")
                        time.sleep(0.02)
                    old = None
                    for index in range(30):
                        command("set", "sid", "no")
                        if old is not None:
                            command("sub-remove", str(old))
                            assert command("get_property", "track-list/count") == 1
                        styled = index % 2 == 0
                        command("set", "sub-ass-override", "no" if styled else "yes")
                        content = url if styled else "memory://WEBVTT\n\n00:00:01.001 --> 00:00:02.999\nPlain\n\n"
                        command("sub-add", content, "select", "VSF accepted subtitles")
                        old = command("get_property", "sid")
                        assert isinstance(old, int) and old > 0
                        assert command("get_property", "track-list/count") == 2
                    command("set", "sid", "no")
                    command("sub-remove", str(old))
                    assert command("get_property", "track-list/count") == 1
                    command("quit")
            child.wait(timeout=10)
            assert child.returncode == 0
            return {"replacements": 30, "maximum_external_tracks": 1, "removed_on_clear": True,
                    "normal_exit": True}
        finally:
            if child.poll() is None:
                child.kill()
            child.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mpv", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = root / "tests/fixtures/generated/quality/ass-native/preview.url"
    url = source.read_text()
    assert url.startswith("hex://"), "Run the Rust production serializer test first"
    args.output.mkdir(parents=True, exist_ok=False)
    receipt = {"scope": "Software composition only; not desktop/GPU/audio acceptance",
               "mpv": str(args.mpv.resolve()),
               "mpv_sha256": hashlib.sha256(args.mpv.read_bytes()).hexdigest(),
               "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
               "version": run([str(args.mpv), "--version"], text=True).stdout,
               "frames": {}}
    white = lambda r, g, b: min(r, g, b) > 180
    red = lambda r, g, b: r > 120 and r > g * 2 and r > b * 2
    for name, seconds in [("early", 1.20), ("late", 1.80), ("red", 2.10), ("clear", 3.10)]:
        destination = args.output / name
        destination.mkdir()
        result = run([str(args.mpv), "--no-config", "--load-scripts=no", "--osc=no", "--ytdl=no",
                      "--vo=image", "--vo-image-format=png", "--vo-image-outdir=" + str(destination),
                      "--ao=null", "--sub-ass-override=no", "--sub-file=" + url,
                      "--start=" + str(seconds), "--frames=1", "av://lavfi:color=c=black:s=768x576:d=6"], text=True)
        # Avoid retaining the long source URL in diagnostic output.
        (destination / "mpv.log").write_text((result.stdout + result.stderr).replace(url, "<VSF ASS source>"))
        png = destination / "00000001.png"
        rgb = run(["ffmpeg", "-v", "error", "-i", str(png), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout
        assert len(rgb) == 768 * 576 * 3, "Unexpected native frame size"
        receipt["frames"][name] = {
            "moving_text": region(rgb, white, 180, 390),
            "white_top": region(rgb, white, 0, 160),
            "red_top": region(rgb, red, 0, 160),
            "nonblack": sum(max(rgb[i:i+3]) > 30 for i in range(0, len(rgb), 3)),
        }
    frames = receipt["frames"]
    assert frames["early"]["white_top"]["pixels"] > 100, "SSA alignment/mixed italic/line break absent"
    assert frames["early"]["moving_text"]["pixels"] > 100, "ASS moving text absent"
    assert frames["late"]["moving_text"]["x"] - frames["early"]["moving_text"]["x"] > 200, "Motion did not advance"
    assert frames["red"]["red_top"]["pixels"] > 100, "ASS color/top alignment absent"
    assert frames["clear"]["nonblack"] == 0, "Expired subtitles remain visible"
    receipt["replacement"] = replacements(args.mpv, url)
    receipt["passed"] = True
    (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("ASS/SSA software-composition checks passed; receipt:", args.output / "receipt.json")


if __name__ == "__main__":
    main()
