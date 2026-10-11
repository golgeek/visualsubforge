#!/usr/bin/env python3
"""Build the pinned ANGLE/Metal SDK on a macOS host of the target architecture.

Uses a new private directory. depot_tools self-update is disabled; ANGLE's
pinned DEPS controls its dependencies. No system library is installed/replaced.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess


ROOT = Path(__file__).resolve().parent.parent


def build(args):
    if platform.system() != "Darwin":
        raise ValueError("The Metal SDK must be built on macOS")
    cpu = {"arm64": "arm64", "x86_64": "x64"}.get(platform.machine())
    if cpu is None:
        raise ValueError(f"Unsupported build architecture: {platform.machine()}")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    pin = json.loads((ROOT / "tests/native/angle/manifest.json").read_text())
    depot = root / "depot_tools"
    source = root / "source"
    env = dict(os.environ, DEPOT_TOOLS_UPDATE="0", DEPOT_TOOLS_METRICS="0")
    env["PATH"] = str(depot) + os.pathsep + env["PATH"]
    receipt = {"schema": 1, "status": "incomplete", "pins": pin,
               "architecture": platform.machine(), "deployment_target": args.minimum_macos,
               "commands": [], "scope": "SDK build; no GPU/audio or distribution acceptance"}
    try:
        def run(command, cwd=None):
            command = list(map(str, command))
            receipt["commands"].append({"args": command, "cwd": str(cwd or root)})
            print("RUN", json.dumps(command), flush=True)
            subprocess.run(command, cwd=cwd or root, env=env, stdin=subprocess.DEVNULL, check=True)

        def clone(url, commit, directory):
            run(["git", "init", directory])
            run(["git", "remote", "add", "origin", url], directory)
            run(["git", "fetch", "--depth=1", "origin", commit], directory)
            run(["git", "checkout", "--detach", "FETCH_HEAD"], directory)
            actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=directory, text=True).strip()
            if actual != commit:
                raise ValueError(f"Unexpected checkout: {actual}")

        clone(pin["depot_tools_url"], pin["depot_tools_commit"], depot)
        clone(pin["upstream_url"], pin["upstream_commit"], source)
        solution = [{"name": ".", "url": pin["upstream_url"], "managed": False,
                     "custom_vars": {"checkout_angle_cl_deps": False,
                                     "checkout_angle_dawn_deps": False,
                                     "checkout_angle_restricted_traces": False}, "custom_deps": {}}]
        (source / ".gclient").write_text("solutions = " + repr(solution) + "\n")
        run([depot / "gclient", "sync", "--no-history", "--nohooks"], source)
        run([depot / "gclient", "runhooks"], source)
        gn_args = (f'is_debug=false target_cpu="{cpu}" mac_deployment_target="{args.minimum_macos}" '
                   f'mac_min_system_version="{args.minimum_macos}" '
                   'angle_enable_gl=false angle_enable_vulkan=false angle_enable_metal=true '
                   'angle_enable_wgpu=false angle_build_tests=false use_system_xcode=true '
                   'use_remoteexec=false symbol_level=0 is_component_build=false enable_rust=false')
        receipt["gn_args"] = gn_args
        run([source / "buildtools/mac/gn", "gen", "out/Metal", "--fail-on-unused-args", "--args=" + gn_args], source)
        run([source / "third_party/ninja/ninja", "-C", "out/Metal", "-j", args.jobs, "libEGL", "libGLESv2"], source)
        sdk = root / "sdk"
        shutil.copytree(source / "include", sdk / "include")
        libraries = sdk / "out/Metal"
        libraries.mkdir(parents=True)
        receipt["binaries"] = []
        for name in ["libEGL.dylib", "libGLESv2.dylib"]:
            path = libraries / name
            shutil.copy2(source / "out/Metal" / name, path)
            receipt["binaries"].append({"file": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "build_version": subprocess.check_output(["xcrun", "vtool", "-show-build", str(path)], text=True)})
        # Retain the resolved dependency revisions without exporting the build
        # environment. The full source remains available in the private job dir.
        revisions = subprocess.check_output([str(depot / "gclient"), "revinfo"], cwd=source, env=env, text=True)
        (sdk / "dependency-revisions.txt").write_text(revisions)
        notices = sdk / "source-notices"
        for directory, children, names in os.walk(source):
            children[:] = [name for name in children if name not in {".git", "out", "__pycache__"}]
            for name in names:
                if name.upper().startswith(("LICENSE", "COPYING", "NOTICE")):
                    path = Path(directory) / name
                    if path.is_file() and not path.is_symlink():
                        target = notices / path.relative_to(source)
                        target.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(path, target)
        receipt["status"] = "built"
        print(f"SDK: {sdk}")
    finally:
        (root / "build-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--minimum-macos", choices=["14.0", "15.0"], default="14.0")
    parser.add_argument("--jobs", type=int, choices=range(1, 17), default=3)
    try:
        build(parser.parse_args())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"ANGLE build failed: {error}\n")
