#!/usr/bin/env python3
"""Exercise the actual NSIS installer on a hosted Windows runner.

This is a fresh install path with a scrubbed PATH, not a clean consumer Windows
image. Native render/audio acceptance remains a separate manual gate.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("package", "installer-dir", "probe", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    installed = args.output.resolve() / "Installed Preview"
    installers = list(args.installer_dir.glob("*.exe"))
    if len(installers) != 1:
        raise ValueError(f"Expected one installer: {installers}")
    # NSIS requires /D last and without quotes, even when the directory has spaces.
    # Pass the exact Windows command line rather than subprocess's argv quoting.
    command = f'"{installers[0].resolve()}" /S /D={installed}'
    result = subprocess.run(command, timeout=300)
    if result.returncode:
        raise ValueError(f"Installer exited {result.returncode}")
    receipt = json.loads((args.package / "assembly-receipt.json").read_text())
    for item in receipt["files"]:
        actual = hashlib.sha256((installed / item["path"]).read_bytes()).hexdigest()
        if actual != item["sha256"]:
            raise ValueError(f"Installer changed or omitted {item['path']}")
    subprocess.run([sys.executable, ROOT / "tests/release/packaged-cli.py", "--bundle", installed,
                    "--bin", ".", "--output", args.output / "cli"], check=True, timeout=450)
    probe_bundle = args.output / "Unload Control"
    shutil.copytree(installed, probe_bundle)
    shutil.copy2(args.probe, probe_bundle / "dll-reload-probe.exe")
    mpv = list(probe_bundle.glob("*mpv*.dll"))
    if len(mpv) != 1:
        raise ValueError(f"Expected one installed mpv DLL: {mpv}")
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("VSF_", "LD_", "DYLD_", "PKG_CONFIG_"))}
    env["PATH"] = str(Path(env["SystemRoot"]) / "System32")
    probe = subprocess.run([probe_bundle / "dll-reload-probe.exe", mpv[0], "reload", "unchanged"],
                           env=env, capture_output=True, text=True, timeout=60)
    (args.output / "dll-unload.log").write_text(probe.stdout + probe.stderr)
    if probe.returncode or "ggml_loaded=1" in probe.stdout or probe.stdout.count("handler_unchanged=1") != 10:
        raise ValueError(f"DLL unload control failed: {probe.returncode}: {probe.stdout}")
    evidence = {"status": "passed", "scope": "Hosted Windows Server 2022 runner; no clean-OS or GPU/audio claim",
                "installer_sha256": hashlib.sha256(installers[0].read_bytes()).hexdigest(),
                "installed_native_files_verified": len(receipt["files"]),
                "dll_unload_cycles": 3, "ggml_loaded": False,
                "cli": "cli/packaged-cli-check.json"}
    (args.output / "install-check.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
