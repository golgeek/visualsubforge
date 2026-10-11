#!/usr/bin/env python3
"""Generate task006's synthetic large project; never overwrite an existing run."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time


def timestamp(milliseconds):
    hours, remainder = divmod(milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    seconds, milliseconds = divmod(remainder, 1_000)
    return f"{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}"


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    output = args.output.resolve()
    commands = []

    def run(argv, timeout=300):
        started = time.monotonic()
        result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        commands.append({"argv": argv, "seconds": time.monotonic() - started,
                         "returncode": result.returncode, "stderr": result.stderr})
        (output / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        result.check_returncode()
        return result.stdout

    versions = {name: run([tool, "-version"]).splitlines()[0]
                for name, tool in [("ffmpeg", args.ffmpeg), ("ffprobe", args.ffprobe)]}
    seed = output / "seed.mkv"
    media = output / "workflow.mkv"
    # Twelve seconds is exactly 360 video frames and 576,000 audio samples.
    # FLAC avoids repeating an AAC encoder-delay/padding boundary in this fixture.
    run([args.ffmpeg, "-hide_banner", "-nostdin", "-n", "-v", "warning",
         "-f", "lavfi", "-i", "testsrc=size=1920x1080:rate=30:duration=12",
         "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=12",
         "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "fast",
         "-crf", "30", "-pix_fmt", "yuv420p", "-g", "60",
         "-c:a", "flac", "-ac", "2", str(seed)])
    run([args.ffmpeg, "-hide_banner", "-nostdin", "-n", "-v", "warning",
         "-stream_loop", "599", "-i", str(seed), "-map", "0", "-c", "copy",
         "-t", "7200", str(media)])
    probe = json.loads(run([args.ffprobe, "-v", "error", "-show_format",
                            "-show_streams", "-of", "json", str(media)]))
    (output / "probe.json").write_text(json.dumps(probe, indent=2) + "\n")
    video = next(stream for stream in probe["streams"] if stream["codec_type"] == "video")
    audio = next(stream for stream in probe["streams"] if stream["codec_type"] == "audio")
    if ((video["width"], video["height"], video["codec_name"]) != (1920, 1080, "h264")
            or audio["channels"] != 2 or audio["codec_name"] != "flac"
            or abs(float(probe["format"]["duration"]) - 7200) > 0.1):
        raise ValueError("Generated media does not match the task006 fixture contract")
    subtitles = output / "workflow.srt"
    with subtitles.open("x", encoding="utf-8", newline="\n") as target:
        for index in range(10_000):
            start = 1_000 + index * 700
            target.write(f"{index + 1}\n{timestamp(start)} --> {timestamp(start + 550)}\n"
                         f"Cue {index + 1:05}: café 日本語\nUnicode editing fixture\n\n")
    manifest = {
        "purpose": "Synthetic task006 editing/memory workload; not real-film or audio precision evidence",
        "generator_sha256": sha256(Path(__file__)), "versions": versions,
        "duration_seconds": 7200, "seed_seconds": 12, "repetitions": 600,
        "cues": 10_000, "cue_start_ms": 1000, "cue_spacing_ms": 700,
        "cue_duration_ms": 550, "last_cue_end_ms": 7_000_850,
        "limitations": ["Repeated synthetic video and continuous tone; no speech or unique scene content",
                        "Dense synthetic cue cadence; not human-authored dialogue",
                        "Container metadata validation does not prove sample continuity at joins"],
        "files": {p.name: {"bytes": p.stat().st_size, "sha256": sha256(p)}
                  for p in [seed, media, subtitles]},
    }
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
