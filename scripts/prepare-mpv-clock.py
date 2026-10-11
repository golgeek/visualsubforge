#!/usr/bin/env python3
"""Prepare pinned mpv rendering and optional audio experiments in isolation.

Copies committed upstream source only. Does not modify that clone, build or
install libraries, or change the application's playback dependency.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


UPSTREAM_COMMIT = "c1529642089bfebfc928a1c1664638a7a5d219ba"
SERIES_SHA256 = "d7aed63acc8ba71d0eddc8c507e4a1c43d33ba0ef0403af6aa4d5f56a28878df"
RENDER_SERIES_SHA256 = "287c958e7f1169f81b46b6b8debf8570a48895e81a93166dfd3315574de340ff"
COREAUDIO_SERIES_SHA256 = "8111e33703c24bfb3a54040aeb86fe171552187e5a4b96a0b20c0e29a4a28c5f"

CANDIDATE_SERIES_SHA256 = "48a4e0d2d753bf52cbaf6855a355a1fc31091fdf1aec1e0d409d74620c138971"


def git(*args, cwd=None):
    return subprocess.run(["git", *map(str, args)], cwd=cwd, check=True,
                          capture_output=True, text=True).stdout.strip()


def read_series(parser, root, directory, checksum):
    patch_root = root / "tests/native" / directory
    series_bytes = (patch_root / "series.json").read_bytes()
    if hashlib.sha256(series_bytes).hexdigest() != checksum:
        parser.error(f"{directory} checksum changed; review the pinned recipe first")
    series = json.loads(series_bytes)
    if series["upstream_commit"] != UPSTREAM_COMMIT:
        parser.error(f"{directory} targets a different upstream commit")
    for entry in series["patches"] + series.get("integrated_upstream", []):
        if hashlib.sha256((patch_root / entry["file"]).read_bytes()).hexdigest() != entry["sha256"]:
            parser.error(f"Patch checksum changed: {directory}/{entry['file']}")
    return {"directory": directory, "sha256": checksum, "patches": series["patches"],
            "integrated_upstream": series.get("integrated_upstream", [])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True,
                        help="Local upstream mpv Git clone containing the pinned commit")
    parser.add_argument("--output", type=Path, required=True,
                        help="New directory for source and provenance manifest")
    parser.add_argument("--render-only", action="store_true",
                        help="Prepare the unmodified pinned upstream base, omitting local audio and CoreAudio patches")
    parser.add_argument("--audio-candidates", action="store_true",
                        help="Include the three unfinished audio corrections for explicit validation")
    args = parser.parse_args()
    if args.render_only and args.audio_candidates:
        parser.error("--audio-candidates cannot be combined with --render-only")
    root = Path(__file__).resolve().parent.parent
    series = [read_series(parser, root, "mpv-render", RENDER_SERIES_SHA256)]
    if not args.render_only:
        series.append(read_series(parser, root, "mpv-clock", SERIES_SHA256))
        series.append(read_series(parser, root, "mpv-coreaudio-cleanup", COREAUDIO_SERIES_SHA256))
    if args.audio_candidates:
        series.append(read_series(parser, root, "mpv-clock/candidates", CANDIDATE_SERIES_SHA256))
    source = args.source.resolve(strict=True)
    # Verify the exact object and its type separately. The MSYS2 Git invocation
    # from native Windows Python did not preserve the ^{commit} suffix.
    commit = git("rev-parse", "--verify", UPSTREAM_COMMIT, cwd=source)
    if commit != UPSTREAM_COMMIT or git("cat-file", "-t", commit, cwd=source) != "commit":
        parser.error("Source does not contain the pinned upstream commit")
    # Retired backports remain auditable, but must already be in this base.
    for group in series:
        for entry in group["integrated_upstream"]:
            try:
                git("merge-base", "--is-ancestor", entry["upstream_fix"], commit, cwd=source)
            except subprocess.CalledProcessError as error:
                if error.returncode == 1:
                    parser.error(f"Required upstream fix is not in the pinned base: {entry['upstream_fix']}")
                raise
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    checkout = output / "source"
    # No hard links or uncommitted changes are shared with the supplied clone.
    git("clone", "--no-hardlinks", "--no-checkout", "--", source, checkout)
    git("checkout", "--detach", UPSTREAM_COMMIT, cwd=checkout)
    for group in series:
        for entry in group["patches"]:
            patch = root / "tests/native" / group["directory"] / entry["file"]
            git("apply", "--check", "--whitespace=error", patch, cwd=checkout)
            git("apply", "--whitespace=error", patch, cwd=checkout)
    (output / "manifest.json").write_text(json.dumps({
        "upstream_url": "https://github.com/mpv-player/mpv",
        "upstream_commit": git("rev-parse", "HEAD", cwd=checkout),
        "series": series,
        "source": str(checkout),
        "status": "prepared; not built, installed or accepted",
    }, indent=2) + "\n")
    print(checkout)


if __name__ == "__main__":
    try:
        main()
    except (OSError, subprocess.CalledProcessError) as error:
        detail = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
        raise SystemExit(f"Unable to prepare mpv experiment: {detail}") from error
