#!/usr/bin/env python3
"""Generate synthetic hard-cut fixtures; run from the repository root."""
import os
from pathlib import Path
import subprocess

output = Path("tests/fixtures/generated")
output.mkdir(parents=True, exist_ok=True)
ffmpeg = os.environ.get("VSF_FFMPEG", "ffmpeg")
for name, variable_rate in [("scene-cfr", False), ("scene-vfr", True)]:
    filters = "[0:v][1:v][2:v]concat=n=3:v=1:a=0,"
    if variable_rate:
        # Keep both exact cut frames while dropping other frames irregularly.
        filters += "select='eq(mod(n,3),0)+eq(n,25)+eq(n,50)',"
    filters += "setpts=PTS+5/TB[v]"
    command = [ffmpeg, "-v", "error", "-y"]
    for color in ["black", "white", "black"]:
        command += ["-f", "lavfi", "-i", f"color={color}:s=320x180:r=25:d=1"]
    command += ["-filter_complex", filters, "-map", "[v]", "-fps_mode",
                "passthrough", "-c:v", "ffv1", str(output / f"{name}.mkv")]
    subprocess.run(command, check=True)
for name, source, codec in [
    ("scene-flat.mkv", "color=black:s=320x180:r=25:d=1", "ffv1"),
    ("scene-audio.wav", "sine=duration=0.2", "pcm_s16le"),
]:
    subprocess.run([ffmpeg, "-v", "error", "-y", "-f", "lavfi", "-i", source,
                    "-c", codec, str(output / name)], check=True)

# H.264 with fixed GOP and B frames: visual cuts deliberately between keyframes.
command = [ffmpeg, "-v", "error", "-y"]
for source in ["testsrc2=s=1280x720:r=25:d=1.12",
               "smptebars=s=1280x720:r=25:d=1.12",
               "color=white:s=1280x720:r=25:d=1.12"]:
    command += ["-f", "lavfi", "-i", source]
command += ["-filter_complex", "[0:v][1:v][2:v]concat=n=3:v=1:a=0,setpts=PTS+5/TB[v]",
            "-map", "[v]", "-fps_mode", "passthrough", "-c:v", "libx264",
            "-threads", "2", "-g", "250", "-keyint_min", "250", "-sc_threshold", "0",
            "-bf", "3", str(output / "scene-fixed-gop.mkv")]
subprocess.run(command, check=True)
# Continuous synthetic motion with periodic keyframes should not turn into cuts.
subprocess.run([ffmpeg, "-v", "error", "-y", "-f", "lavfi", "-i",
                "testsrc2=s=720x1280:r=25:d=3", "-c:v", "libx264", "-threads", "2",
                "-g", "25", "-keyint_min", "25", "-sc_threshold", "0",
                str(output / "scene-motion-portrait.mkv")], check=True)

# Scene-driven non-key I pictures, including irregular presentation intervals
# and a nonzero container origin. Keep GOP minimum above the cut positions.
for rate in ["cfr", "vfr"]:
    subprocess.run([ffmpeg, "-v", "error", "-y", "-copyts", "-i",
                    str(output / f"scene-{rate}.mkv"), "-c:v", "libx264",
                    "-threads", "2", "-g", "250", "-keyint_min", "250",
                    "-sc_threshold", "40", "-fps_mode", "passthrough",
                    str(output / f"scene-intra-{rate}.mkv")], check=True)
