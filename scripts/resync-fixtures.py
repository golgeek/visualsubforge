#!/usr/bin/env python3
"""Generate resync ground truth from edit lists, independently of any matcher.

Outputs are disposable, under tests/fixtures/generated/resync. --reference uses
the existing licensed Blender fixture; no external media is downloaded here.
"""
import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests/fixtures/generated/resync"
FFMPEG = os.environ.get("VSF_FFMPEG", "ffmpeg")
FFPROBE = os.environ.get("VSF_FFPROBE", "ffprobe")
COMMANDS = []


def run(command):
    COMMANDS.append(command)
    result = subprocess.run(command, capture_output=True, timeout=300)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace")[-16_384:])
    return result.stdout


def encode(arguments, path):
    run([FFMPEG, "-v", "error", "-nostdin", "-y", *arguments,
         "-an", "-c:v", "libx264", "-threads", "2", "-preset", "veryfast",
         "-crf", "18", "-pix_fmt", "yuv420p", str(path)])


def micros(value):
    value = Fraction(value) * 1_000_000
    return (value.numerator * 2 + value.denominator) // (2 * value.denominator)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def synthetic(path, seconds=36, fps=24, seed=100, repeat=False, flat=False):
    """Original spatial textures and moving objects; no embedded shot IDs.

    Every shot lasts two seconds deliberately: cut intervals alone cannot
    distinguish these clips or repeated shots.
    """
    width, height = 96, 64
    with tempfile.TemporaryFile() as raw:
        backgrounds = {}
        for frame in range(seconds * fps):
            shot = frame // (2 * fps)
            identity = shot % 4 if repeat else shot
            if flat:
                identity = 0
            if identity not in backgrounds:
                rng = random.Random(seed + identity)
                low, high = (15, 105) if identity % 2 == 0 else (145, 235)
                grid = [rng.randrange(low, high) for _ in range(12 * 8)]
                backgrounds[identity] = bytes(
                    grid[(y // 8) * 12 + x // 8]
                    for y in range(height) for x in range(width) for _ in range(3)
                )
            pixels = bytearray(backgrounds[identity])
            if not flat:
                x = (frame % (2 * fps)) * 2 % (width - 8)
                for y in range(24, 32):
                    offset = (y * width + x) * 3
                    pixels[offset:offset + 24] = bytes([255, 90, 50]) * 8
            raw.write(pixels)
        raw.seek(0)
        command = [FFMPEG, "-v", "error", "-y", "-f", "rawvideo",
                   "-pixel_format", "rgb24", "-video_size", f"{width}x{height}",
                   "-framerate", str(fps), "-i", "pipe:0", "-an", "-c:v", "ffv1",
                   "-threads", "2", str(path)]
        COMMANDS.append(command)
        subprocess.run(command, stdin=raw, check=True, capture_output=True, timeout=300)


def edited(source, name, pieces, *, scale=Fraction(1), fps=24, vf=None):
    """A piece is (source start, source end) or an unmatched ad duration.

    Truth is calculated directly from this rational edit list, never inferred
    from scene detection, signatures or a produced alignment.
    """
    filters, inputs, truth = [], [], []
    target = Fraction(0)
    for index, piece in enumerate(pieces):
        label = f"p{index}"
        if isinstance(piece, tuple):
            start, end = map(Fraction, piece)
            duration = (end - start) * scale
            filters.append(
                f"[0:v]trim=start={float(start)}:end={float(end)},"
                f"setpts=(PTS-STARTPTS)*{scale.numerator}/{scale.denominator},"
                f"fps={fps},settb=AVTB[{label}]"
            )
            truth.append({
                "source_start_us": micros(start), "source_end_us": micros(end),
                "target_start_us": micros(target), "target_end_us": micros(target + duration),
            })
        else:
            duration = Fraction(piece)
            filters.append(f"testsrc2=s=96x64:r={fps}:d={float(duration)},"
                           f"settb=AVTB[{label}]")
        target += duration
        inputs.append(f"[{label}]")
    filters.append("".join(inputs) + f"concat=n={len(pieces)}:v=1:a=0[joined]")
    filters.append(f"[joined]{vf or 'null'}[out]")
    output = OUT / f"{name}.mkv"
    encode(["-i", str(source), "-filter_complex", ";".join(filters),
            "-map", "[out]", "-fps_mode", "passthrough"], output)
    return {"name": name, "source": source.name, "target": output.name,
            "segments": truth, "target_duration_us": micros(target),
            "expected": "map_supported_interiors"}


def probe(path):
    data = json.loads(run([FFPROBE, "-v", "error", "-select_streams", "v:0",
                          "-show_entries", "frame=best_effort_timestamp_time:format=start_time,duration",
                          "-show_frames", "-of", "json", str(path)]))
    times = [Fraction(frame["best_effort_timestamp_time"]) for frame in data["frames"]]
    if not times or any(b <= a for a, b in zip(times, times[1:])):
        raise RuntimeError(f"Missing or non-monotonic presentation timestamps: {path}")
    gaps = [b - a for a, b in zip(times, times[1:])]
    return {"sha256": digest(path), "frames": len(times),
            "origin_us": micros(data["format"].get("start_time", "0")),
            "duration_us": micros(data["format"]["duration"]),
            "max_frame_interval_us": micros(max(gaps, default=Fraction(0)))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", action="store_true",
                        help="also derive edits from the already fetched Blender reference movie")
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    source = OUT / "source.mkv"
    synthetic(source)
    ad_edits = [1, (2, 12), 2, (14, 24), 3, (Fraction(97, 4), 34)]
    cases = [
        edited(source, "offset", [3, (0, 36), 2]),
        edited(source, "fps-only", [(0, 36)], fps=30),
        edited(source, "speed", [(0, 36)], scale=Fraction(24, 25), fps=25),
        edited(source, "ads-trims", ad_edits),
        edited(source, "speed-ads", ad_edits, scale=Fraction(24, 25), fps=25),
        edited(source, "vfr-origin", [(0, 36)],
               vf="select='not(mod(n,3))',setpts=PTS+5/TB"),
    ]
    # Same cut times and durations, completely different appearance.
    synthetic(OUT / "unrelated.mkv", seed=8000)
    cases.append({"name": "unrelated", "source": source.name, "target": "unrelated.mkv",
                  "segments": [], "expected": "abstain"})
    synthetic(OUT / "repeated.mkv", repeat=True)
    cases.append({"name": "repeated", "source": "repeated.mkv", "target": "repeated.mkv",
                  "segments": [], "expected": "abstain"})
    synthetic(OUT / "flat.mkv", flat=True)
    cases.append({"name": "flat", "source": "flat.mkv", "target": "flat.mkv",
                  "segments": [], "expected": "abstain"})
    reordered = edited(source, "reordered", [(24, 36), (0, 12), (12, 24)])
    reordered["expected"] = "reject_reordered"
    cases.append(reordered)
    if args.reference:
        reference = ROOT / "tests/fixtures/generated/big-buck-bunny.mp4"
        if not reference.is_file():
            raise RuntimeError("Run python3 scripts/fetch-reference-movie.py --resolution 180 first")
        encode(["-i", str(reference), "-vf", "scale=96:64,setsar=1", "-fps_mode", "passthrough"],
               OUT / "reference.mkv")
        cases.append(edited(OUT / "reference.mkv", "reference-ads",
                            [1, (30, 200), 2, (215, 400), 3, (Fraction(1609, 4), 550)]))
    media = {name: probe(OUT / name) for name in sorted(
        {case[key] for case in cases for key in ("source", "target")})}
    for case in cases:
        source_info, target_info = (media[case[key]] for key in ("source", "target"))
        case["boundary_tolerance_us"] = (source_info["max_frame_interval_us"]
                                         + target_info["max_frame_interval_us"] + 1_000)
        if "target_duration_us" in case:
            # Matroska duration includes the nonzero timestamp origin.
            observed = target_info["duration_us"] - target_info["origin_us"]
            if abs(observed - case["target_duration_us"]) > case["boundary_tolerance_us"]:
                raise RuntimeError(f"Encoded duration disagrees with edit list: {case['name']}")
    manifest = {"schema_version": 1, "cases": cases, "media": media,
                "tools": {name: run([tool, "-version"]).decode().splitlines()[0]
                          for name, tool in [("ffmpeg", FFMPEG), ("ffprobe", FFPROBE)]},
                "reference_attribution": "Blender Foundation, Big Buck Bunny (2008), CC BY 3.0; "
                                         "derived edits for testing" if args.reference else None,
                "commands": COMMANDS}
    (OUT / "truth.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Generated {len(cases)} cases in {OUT}; presentation timing and durations verified.")


if __name__ == "__main__":
    main()
