#!/usr/bin/env python3
"""Generate resync ground truth from edit lists, independently of any matcher.

Outputs are disposable, under tests/fixtures/generated (resync by default).
--reference and --live-action use existing licensed Blender fixtures; no
external media is downloaded here.
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


def synthetic(path, seconds=36, fps=24, seed=100, repeat=False, flat=False, still_tail=None,
              fade_every=None, shot_seconds=2, pan=False):
    """Original spatial textures and moving objects; no embedded shot IDs.

    Default shots last two seconds: cut intervals alone cannot distinguish
    these clips or repeated shots. Some robustness cases use longer shots.
    """
    width, height = 96, 64
    if pan and fade_every:
        raise ValueError("Use separate pan and fade fixtures")
    field_width = width + shot_seconds * fps if pan else width
    columns = (field_width + 7) // 8
    with tempfile.TemporaryFile() as raw:
        backgrounds = {}
        for frame in range(seconds * fps):
            shot = frame // (shot_seconds * fps)
            identity = shot % 4 if repeat else shot
            still = still_tail is not None and frame >= still_tail * fps
            if still:
                identity = still_tail // shot_seconds
            if flat:
                identity = 0
            if identity not in backgrounds:
                rng = random.Random(seed + identity)
                low, high = (15, 105) if identity % 2 == 0 else (145, 235)
                grid = [rng.randrange(low, high) for _ in range(columns * 8)]
                backgrounds[identity] = bytes(
                    grid[(y // 8) * columns + x // 8]
                    for y in range(height) for x in range(field_width) for _ in range(3)
                )
            phase = frame % (shot_seconds * fps)
            pixels = bytearray(backgrounds[identity])
            if pan:
                pixels = bytearray().join(
                    backgrounds[identity][(y * field_width + phase) * 3:
                                          (y * field_width + phase + width) * 3]
                    for y in range(height)
                )
            if fade_every and shot and shot % fade_every == 0:
                phase = frame % (shot_seconds * fps)
                fade_frames = fps // 2
                if phase < fade_frames:
                    previous = backgrounds[identity - 1]
                    pixels = bytearray((old * (fade_frames - phase) + new * phase) // fade_frames
                                       for old, new in zip(previous, pixels))
            if not flat and not still:
                x = (frame % (shot_seconds * fps)) * 2 % (width - 8)
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


def edited(source, name, pieces, *, scale=Fraction(1), fps=24, vf=None, ad_size="96x64"):
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
            filters.append(f"testsrc2=s={ad_size}:r={fps}:d={float(duration)},"
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
                          "-show_entries", "frame=best_effort_timestamp_time,duration_time:format=start_time,duration",
                          "-of", "json", str(path)]))
    times = [Fraction(frame["best_effort_timestamp_time"]) for frame in data["frames"]]
    if not times or any(b <= a for a, b in zip(times, times[1:])):
        raise RuntimeError(f"Missing or non-monotonic presentation timestamps: {path}")
    gaps = [b - a for a, b in zip(times, times[1:])]
    origin = Fraction(data["format"].get("start_time", "0"))
    last_duration = Fraction(data["frames"][-1].get("duration_time", "0"))
    if last_duration <= 0:
        raise RuntimeError(f"Missing final decoded frame duration: {path}")
    return {"sha256": digest(path), "frames": len(times),
            "origin_us": micros(origin),
            "duration_us": micros(data["format"]["duration"]),
            "first_pts_us": micros(times[0]), "last_pts_us": micros(times[-1]),
            "presentation_duration_us": micros(times[-1] + last_duration - origin),
            "max_frame_interval_us": micros(max(gaps, default=Fraction(0)))}


def main():
    global OUT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", action="store_true",
                        help="also derive edits from the already fetched Blender reference movie")
    parser.add_argument("--extended", action="store_true",
                        help="add held-out seeds, ambiguous static edits and three ten-minute blocks")
    parser.add_argument("--robustness", action="store_true",
                        help="add negative PTS, fractional rates, fades, crops, logos and brightness changes")
    parser.add_argument("--live-action", action="store_true",
                        help="derive held-out edits from the already fetched Tears of Steel movie")
    parser.add_argument("--output", type=Path, default=OUT,
                        help="generated fixture directory (defaults to tests/fixtures/generated/resync)")
    args = parser.parse_args()
    OUT = args.output.resolve()
    if not OUT.is_relative_to((ROOT / "tests/fixtures/generated").resolve()):
        parser.error("--output must be inside tests/fixtures/generated")
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
    if args.extended or args.robustness:
        low_cut = OUT / "low-cut-source.mkv"
        synthetic(low_cut, seconds=120, fps=25, seed=631895, shot_seconds=10)
        # Removing one full motion cycle hides the edit from sparse sampling.
        # No later cut anchors disambiguate the final shot.
        ambiguous = edited(low_cut, "low-cut-tail-cycle-deletion",
                           [(0, 113), (Fraction(2869, 25), 120)], fps=25)
        ambiguous.update(expected="reject_ambiguous_tail", unsafe_after_us=110_000_000)
        cases.append(ambiguous)
        ambiguous_head = edited(low_cut, "low-cut-head-cycle-deletion",
                                [(0, 3), (Fraction(119, 25), 120)], fps=25)
        ambiguous_head.update(expected="reject_ambiguous_head", unsafe_before_us=10_000_000)
        cases.append(ambiguous_head)
    if args.extended:
        held_out = OUT / "held-out-source.mkv"
        synthetic(held_out, seconds=72, fps=30, seed=424242)
        cases.append(edited(held_out, "held-out-speed-ads", [2, (1, 23), 1,
                            (Fraction(49, 2), Fraction(93, 2)), 3,
                            (Fraction(99, 2), Fraction(143, 2))], scale=Fraction(24, 25), fps=25))
        cases.append(edited(held_out, "held-out-resize", [(0, 72)], fps=25, vf="scale=192:128"))
        cases.append(edited(source, "held-out-short-replacement",
                            [(0, Fraction(25, 4)), Fraction(1, 4), (Fraction(13, 2), 36)]))
        still = OUT / "held-out-still-source.mkv"
        synthetic(still, seed=76123, still_tail=12)
        ambiguous = edited(still, "held-out-static-deletion", [(0, 18), (20, 36)])
        ambiguous.update(expected="reject_ambiguous_tail", unsafe_after_us=12_000_000)
        cases.append(ambiguous)
        long_source = OUT / "long-source.mkv"
        synthetic(long_source, seconds=1860, seed=45678)
        cases.append(edited(long_source, "long-speed-ads", [5, (4, 604), 3, (620, 1220), 7, (1240, 1840)],
                            scale=Fraction(24, 25), fps=25))
    if args.reference:
        reference = ROOT / "tests/fixtures/generated/big-buck-bunny.mp4"
        if not reference.is_file():
            raise RuntimeError("Run python3 scripts/fetch-reference-movie.py --resolution 180 first")
        encode(["-i", str(reference), "-vf", "scale=96:64,setsar=1", "-fps_mode", "passthrough"],
               OUT / "reference.mkv")
        cases.append(edited(OUT / "reference.mkv", "reference-ads",
                            [1, (30, 200), 2, (215, 400), 3, (Fraction(1609, 4), 550)]))
    if args.robustness:
        # Expectations are selected before inspecting any matcher result. Ordinary
        # rate/origin conversions retain the 90% gate; visual distortions measure
        # scope but must never accept incorrect correspondence at any coverage.
        held_out = OUT / "robust-source.mkv"
        synthetic(held_out, seconds=72, fps=25, seed=909019)
        edits = [2, (1, 23), 3, (Fraction(49, 2), 47), 1, (50, 71)]
        cases.append(edited(held_out, "fractional-fps-only", edits, fps="24000/1001"))
        cases.append(edited(held_out, "fractional-slowdown", edits,
                            scale=Fraction(1001, 960), fps="24000/1001"))
        for name, effect in [
            ("crop", "crop=88:56:4:4,scale=96:64"),
            ("logo", "drawbox=x=78:y=3:w=14:h=10:color=white@0.7:t=fill"),
            ("brightness", "eq=brightness=0.1:contrast=1.12"),
        ]:
            case = edited(held_out, f"robust-{name}", edits, fps=25, vf=effect)
            case["expected"] = "safe_partial_mapping"
            cases.append(case)
        fades = OUT / "fade-source.mkv"
        synthetic(fades, seconds=72, fps=25, seed=672031, fade_every=3)
        case = edited(fades, "robust-fades", edits, scale=Fraction(1001, 960), fps="24000/1001")
        case["expected"] = "safe_partial_mapping"
        cases.append(case)
        # A three-frame deletion inside a moving shot may not trigger scdet.
        # It still must not be hidden by visually similar half-second samples.
        cases.append(edited(low_cut, "low-cut-short-deletion",
                            [(0, Fraction(287, 5)), (Fraction(1438, 25), 120)], fps=25))
        # The first/last frames and total duration still agree. Intervening
        # replacements must prevent video-edge evidence from bridging an edit.
        for name, start in [("low-cut-head-replacement", 3), ("low-cut-tail-replacement", 113)]:
            cases.append(edited(low_cut, name,
                                [(0, start), Fraction(3, 25), (start + Fraction(3, 25), 120)], fps=25))
        # A separate held-out low-cut control has a nonrepeating pan. Retain the
        # original periodic-motion failure above; do not change its truth/gate.
        pan = OUT / "low-cut-pan-source.mkv"
        synthetic(pan, seconds=120, fps=25, seed=852891, shot_seconds=10, pan=True)
        cases.append(edited(pan, "low-cut-pan-deletion",
                            [(0, Fraction(287, 5)), (Fraction(1438, 25), 120)], fps=25))
        cases.append(edited(pan, "low-cut-pan-tail-deletion",
                            [(0, 113), (Fraction(2828, 25), 120)], fps=25))
        negative = OUT / "negative-origin.ts"
        # Matroska/NUT cannot retain these negative presentation timestamps.
        # MPEG-TS can; probe below verifies that they really survived encoding.
        encode(["-i", str(source), "-output_ts_offset", "-5", "-avoid_negative_ts", "disabled",
                "-mpegts_copyts", "1", "-muxdelay", "0", "-f", "mpegts"], negative)
        cases.append({"name": "negative-origin", "source": source.name, "target": negative.name,
                      "segments": [{"source_start_us": 0, "source_end_us": 36_000_000,
                                    "target_start_us": 0, "target_end_us": 36_000_000}],
                      "target_duration_us": 36_000_000, "expected": "map_supported_interiors"})
    if args.live_action:
        reference = ROOT / "tests/fixtures/generated/tears-of-steel-720.mov"
        if not reference.is_file():
            raise RuntimeError("Run python3 scripts/fetch-reference-movie.py --film tears-of-steel first")
        live = OUT / "live-action-source.mkv"
        encode(["-i", str(reference), "-vf", "scale=320:180,setsar=1,fps=25"], live)
        cases.append(edited(live, "live-action-speed-ads",
                            [2, (20, 210), 3, (Fraction(859, 4), 410), 1, (421, 650)],
                            scale=Fraction(1001, 960), fps="24000/1001", ad_size="320x180",
                            vf="scale=480:270"))
    media = {name: probe(OUT / name) for name in sorted(
        {case[key] for case in cases for key in ("source", "target")})}
    for case in cases:
        source_info, target_info = (media[case[key]] for key in ("source", "target"))
        case["boundary_tolerance_us"] = (source_info["max_frame_interval_us"]
                                         + target_info["max_frame_interval_us"] + 1_000)
        if "target_duration_us" in case:
            # Container duration semantics differ. Measure the final decoded
            # frame endpoint relative to origin instead of guessing from format.
            observed = target_info["presentation_duration_us"]
            if abs(observed - case["target_duration_us"]) > case["boundary_tolerance_us"]:
                raise RuntimeError(f"Encoded duration disagrees with edit list: {case['name']}")
    if args.robustness and (media["negative-origin.ts"]["origin_us"] != -5_000_000
                            or media["negative-origin.ts"]["first_pts_us"] != -5_000_000):
        raise RuntimeError("MPEG-TS did not preserve the expected negative presentation origin")
    manifest = {"schema_version": 1, "cases": cases, "media": media,
                "tools": {name: run([tool, "-version"]).decode().splitlines()[0]
                          for name, tool in [("ffmpeg", FFMPEG), ("ffprobe", FFPROBE)]},
                "reference_attribution": "Blender Foundation, Big Buck Bunny (2008), CC BY 3.0; "
                                         "derived edits for testing" if args.reference else None,
                "live_action_attribution": "(CC) Blender Foundation | mango.blender.org; "
                                           "Tears of Steel, CC BY 3.0; silent derived edits for testing"
                                           if args.live_action else None,
                "commands": COMMANDS}
    (OUT / "truth.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Generated {len(cases)} cases in {OUT}; presentation timing and durations verified.")


if __name__ == "__main__":
    main()
