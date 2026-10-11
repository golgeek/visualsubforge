#!/usr/bin/env python3
"""Build/run the isolated macOS composition probe using an existing ANGLE build.

This does not download dependencies or produce a redistributable application.
The generated app links to absolute development library paths. See task 003c.
"""
import argparse
import hashlib
import json
import plistlib
from pathlib import Path
import shlex
import subprocess
from composition_fixture import decoded_peaks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--angle", type=Path, required=True, help="ANGLE checkout with out/Metal libraries")
    parser.add_argument("--media", type=Path, required=True)
    parser.add_argument("--subtitles", type=Path, required=True)
    parser.add_argument("--build-only", action="store_true")
    parser.add_argument("--output", type=Path, help="Diagnostic .app path (default: generated/AngleCompositionProbe.app)")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    angle = args.angle.resolve(strict=True)
    media = args.media.resolve(strict=True)
    subtitles = args.subtitles.resolve(strict=True)
    libraries = angle / "out/Metal"
    for name in ("libEGL.dylib", "libGLESv2.dylib"):
        (libraries / name).resolve(strict=True)
    app = (args.output or root / "tests/fixtures/generated/AngleCompositionProbe.app").resolve()
    contents = app / "Contents"
    (contents / "MacOS").mkdir(parents=True, exist_ok=True)
    resources = contents / "Resources"
    resources.mkdir(exist_ok=True)
    peaks, samples = decoded_peaks(media)
    if not peaks:
        parser.error("The waveform probe requires an audio stream with decoded samples")
    page = (root / "tests/native/angle-composition.html").read_text()
    page = page.replace("__VSF_PEAKS__", json.dumps(peaks))
    page = page.replace("__VSF_PEAK_DURATION__", json.dumps(samples / 48000))
    (resources / "composition.html").write_text(page, encoding="utf-8")
    binary = contents / "MacOS/AngleCompositionProbe"
    (contents / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleIdentifier": "com.golgeek.vsf-angle-composition-probe",
        "CFBundleName": "AngleCompositionProbe",
        "CFBundleExecutable": "AngleCompositionProbe",
        "CFBundlePackageType": "APPL", "NSHighResolutionCapable": True,
    }))
    mpv = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "mpv"], text=True))
    subprocess.run(["clang++", "-std=c++17", "-fobjc-arc", "-Wall", "-Wextra", "-Werror",
                    f"-I{angle / 'include'}", str(root / "tests/native/angle-composition-probe.mm"),
                    f"-L{libraries}", "-lEGL", "-lGLESv2", *mpv,
                    "-framework", "AppKit", "-framework", "WebKit", "-framework", "QuartzCore",
                    "-o", str(binary)], check=True)
    subprocess.run(["install_name_tool", "-change", "./libEGL.dylib", str(libraries / "libEGL.dylib"),
                    "-change", "./libGLESv2.dylib", str(libraries / "libGLESv2.dylib"), str(binary)], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
    app.with_suffix(".manifest.json").write_text(json.dumps({
        "media": str(media), "subtitles": str(subtitles),
        "decoded_samples": samples, "sample_rate": 48000, "peak_bins": len(peaks),
        "files": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (
            media, subtitles, binary, resources / "composition.html",
            root / "tests/native/angle-composition-probe.mm",
            root / "tests/native/angle-composition.html", Path(__file__).resolve(),
            root / "scripts/composition_fixture.py")},
    }, indent=2) + "\n")
    if args.build_only:
        return 0
    return subprocess.run([str(binary), str(media), str(subtitles)]).returncode


if __name__ == "__main__":
    raise SystemExit(main())
