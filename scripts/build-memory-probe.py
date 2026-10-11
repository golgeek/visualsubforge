#!/usr/bin/env python3
"""Build the macOS-only native diagnostic with the installed mpv headers."""
import pathlib
import shlex
import subprocess
import sys

if sys.platform != "darwin":
    raise SystemExit("This diagnostic requires macOS/AppKit.")
root = pathlib.Path(__file__).resolve().parent.parent
output = root / "tests/fixtures/generated/memory-probe"
output.parent.mkdir(parents=True, exist_ok=True)
flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "mpv"], text=True))
command = ["xcrun", "clang++", "-std=c++17", "-fobjc-arc", "-g", "-O1",
           "-Wno-deprecated-declarations",
           str(root / "tests/native/memory-probe.mm"),
           "-framework", "AppKit", "-framework", "QuartzCore", "-framework", "OpenGL",
           *flags, "-o", str(output)]
print(shlex.join(command), flush=True)
subprocess.run(command, check=True)

# Only this disposable diagnostic binary permits debugger attachment. The
# application's signing configuration and system security remain unchanged.
subprocess.run(["codesign", "--force", "--sign", "-", "--entitlements",
                str(root / "tests/native/profiling.entitlements.plist"), str(output)], check=True)
