#!/usr/bin/env python3
"""Stage the isolated Windows composition probe; no downloads or VM changes.

Requires host ffmpeg and the official Microsoft.Web.WebView2 1.0.4258.31
NuGet archive. Copy the output directory to Windows and follow task 003c.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile
from composition_fixture import decoded_peaks


SDK_SHA256 = "56f7f4b8bf9aee4b8efefbbdd4f67d5f74ebd1b100ed0806da71bf76af481aa9"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True, help="New staging directory")
    args = parser.parse_args()
    if hashlib.sha256(args.sdk.read_bytes()).hexdigest() != SDK_SHA256:
        parser.error("SDK archive differs from the pinned official package")
    root = Path(__file__).resolve().parent.parent
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    shutil.copy(root / "tests/native/windows-composition-probe.cs", output)
    with zipfile.ZipFile(args.sdk) as archive:
        for name in ("Microsoft.Web.WebView2.Core.dll", "Microsoft.Web.WebView2.WinForms.dll"):
            (output / name).write_bytes(archive.read("lib/net462/" + name))
        for architecture in ("arm64", "x64", "x86"):
            target = output / architecture
            target.mkdir()
            (target / "WebView2Loader.dll").write_bytes(archive.read(
                "runtimes/win-" + architecture + "/native/WebView2Loader.dll"))
    media = output / "composition.mp4"
    subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
        "testsrc=size=640x360:rate=10:duration=6", "-f", "lavfi", "-i",
        "aevalsrc=0.2*sin(2*PI*440*t)*(0.2+0.8*abs(sin(2*PI*0.7*t))):s=48000:d=6",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "30", "-c:a", "aac",
        "-b:a", "96k", "-shortest", str(media),
    ], check=True)
    peaks, sample_count = decoded_peaks(media)
    page = (root / "tests/native/windows-composition.html").read_text()
    (output / "composition.html").write_text(page.replace("__VSF_PEAKS__", json.dumps(peaks)))
    (output / "preview.srt").write_text(
        "1\n00:00:00,200 --> 00:00:02,400\nGPU preview: café — 日本語\nFirst cue · two lines\n\n"
        "2\n00:00:03,000 --> 00:00:05,800\nSecond cue\nSeek across the gap\n", encoding="utf-8")
    (output / "build.ps1").write_text(r'''$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
& "$env:WINDIR\Microsoft.NET\Framework64\v4.0.30319\csc.exe" /nologo /target:winexe /platform:anycpu /out:VSFComposition.exe /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.Web.Extensions.dll /reference:Microsoft.Web.WebView2.Core.dll /reference:Microsoft.Web.WebView2.WinForms.dll windows-composition-probe.cs
exit $LASTEXITCODE
''')
    (output / "manifest.json").write_text(json.dumps({
        "sdk_version": "1.0.4258.31", "sdk_sha256": SDK_SHA256,
        "decoded_samples": sample_count, "peak_bins": len(peaks),
        "files": {str(path.relative_to(output)): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in sorted(output.rglob("*")) if path.is_file()},
    }, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
