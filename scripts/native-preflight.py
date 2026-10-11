#!/usr/bin/env python3
"""Validate native development inputs and record their identity before a clean build.

This checks selected files, not ABI compatibility or what an application loaded.
Keep the subsequent compiler/linker log and separate runtime evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
from datetime import datetime, timezone
from native_evidence import file_identity as identity

ROOT = Path(__file__).resolve().parent.parent


def capture(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=60).strip()


def sdk(name):
    value = os.environ.get(name)
    if not value:
        raise ValueError(f"Set {name}; see docs/native-runtime.md")
    return Path(value).resolve(strict=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--feature", choices=["media-prototype", "macos-angle"], required=True)
    parser.add_argument("--mpv-runtime", type=Path, required=True, help="Exact development libmpv library/DLL")
    parser.add_argument("--output", type=Path, required=True, help="New JSON receipt")
    parser.add_argument("--application", type=Path, help="Also hash an existing executable; does not validate it")
    parser.add_argument("--mpv-build-receipt", type=Path, help="Receipt from check-mpv-build.py")
    args = parser.parse_args()
    host = platform.system()
    if args.output.exists():
        parser.error("Output exists; keep old evidence and select a new path")
    if host not in {"Darwin", "Linux", "Windows"}:
        parser.error("Native adapter preflight supports macOS, Linux and Windows hosts")
    if args.feature == "macos-angle" and host != "Darwin":
        parser.error("macos-angle requires a macOS host")
    files = [args.mpv_runtime, ROOT / "Cargo.lock", ROOT / "package-lock.json",
             ROOT / "rust-toolchain.toml", Path(__file__), ROOT / "scripts/native_evidence.py"]
    receipt = {
        "schema": 1, "scope": "File preflight only; no compile/link/runtime acceptance",
        "recorded_utc": datetime.now(timezone.utc).isoformat(),
        "host": {"system": host, "release": platform.release(), "machine": platform.machine()},
        "feature": args.feature, "rustc": capture("rustc", "-vV"),
        "application_commit": capture("git", "-C", str(ROOT), "rev-parse", "HEAD"),
        "selection": {key: os.environ.get(key) for key in ["VSF_ANGLE_SDK", "VSF_MPV_SDK",
                      "PKG_CONFIG_PATH", "DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH", "CC", "CXX"]},
    }
    if host == "Windows":
        mpv = sdk("VSF_MPV_SDK")
        files += [mpv / "include/mpv/client.h", mpv / "lib/mpv.lib"]
    else:
        packages = ["mpv"] + (["gtk+-3.0", "epoxy"] if host == "Linux" else [])
        receipt["pkg_config"] = {
            name: {"version": capture("pkg-config", "--modversion", name),
                   "flags": capture("pkg-config", "--cflags", "--libs", name)}
            for name in packages
        }
        include_flags = shlex.split(capture("pkg-config", "--cflags-only-I", "mpv"))
        includes = [Path(flag[2:]) for flag in include_flags if flag.startswith("-I")]
        # pkg-config may omit a system include directory from cflags.
        includes.append(Path(capture("pkg-config", "--variable=includedir", "mpv")))
        for header in ["client.h", "render.h", "render_gl.h"]:
            found = next((p / "mpv" / header for p in includes if (p / "mpv" / header).is_file()), None)
            if found is None:
                raise ValueError(f"Selected mpv pkg-config headers lack mpv/{header}")
            files.append(found)
        # An uninstalled .pc file can retain its future install libdir while
        # emitting -L for the build directory. Check the actual link search.
        link_flags = shlex.split(capture("pkg-config", "--libs-only-L", "mpv"))
        libdirs = [Path(flag[2:]).resolve() for flag in link_flags if flag.startswith("-L")]
        if not libdirs:
            libdirs = [Path(capture("pkg-config", "--variable=libdir", "mpv")).resolve()]
        if args.mpv_runtime.resolve().parent not in libdirs:
            raise ValueError(f"Runtime is outside the selected pkg-config link directories ({libdirs})")
    if host == "Darwin":
        receipt["xcode"] = capture("xcodebuild", "-version")
        receipt["sdk"] = capture("xcrun", "--show-sdk-version")
        receipt["clang"] = capture("xcrun", "clang", "--version")
    if args.feature == "macos-angle":
        angle = sdk("VSF_ANGLE_SDK")
        files += [angle / name for name in ["include/EGL/egl.h", "include/EGL/eglext.h",
                  "include/GLES2/gl2.h", "include/GLES2/gl2ext.h", "include/KHR/khrplatform.h",
                  "out/Metal/libEGL.dylib", "out/Metal/libGLESv2.dylib"]]
        if (angle / "headers-manifest.json").is_file():
            files.append(angle / "headers-manifest.json")
    if args.application:
        files.append(args.application)
    if args.mpv_build_receipt:
        build = json.loads(args.mpv_build_receipt.read_text())
        if build.get("status") != "passed":
            raise ValueError("mpv build receipt is not passing")
        runtime = identity(args.mpv_runtime)
        if not any(b["sha256"] == runtime["sha256"] for b in build["binaries"]):
            raise ValueError("Runtime bytes differ from the supplied build receipt")
        files.append(args.mpv_build_receipt)
    receipt["files"] = [identity(path) for path in files]
    changes = subprocess.check_output(["git", "-C", str(ROOT), "diff", "--binary", "HEAD"])
    receipt["tracked_changes_sha256"] = hashlib.sha256(changes).hexdigest()
    receipt["tracked_changes_present"] = bool(changes)
    # New output is created only after every selected input has validated.
    with args.output.open("x") as stream:
        json.dump(receipt, stream, indent=2)
        stream.write("\n")
    print(f"PASS: native files present; receipt {args.output}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        raise SystemExit(f"Native preflight failed: {error}") from error
