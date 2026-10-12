#!/usr/bin/env python3
"""Assemble a private Windows x64 runtime and fail closed on unknown DLLs.

The Windows API set and an explicit list of OS DLLs remain platform components.
Compiler runtimes must be bundled or statically linked; finding a DLL somewhere
on a development machine never makes it an operating-system dependency.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

from native_evidence import digest

ROOT = Path(__file__).resolve().parent.parent
OS_DLLS = set("""
advapi32.dll avrt.dll bcrypt.dll cabinet.dll cfgmgr32.dll comctl32.dll comdlg32.dll
crypt32.dll cryptbase.dll cryptnet.dll cryptsp.dll d2d1.dll d3d11.dll d3d12.dll
d3d9.dll d3dcompiler_47.dll dbghelp.dll dnsapi.dll dsound.dll dwmapi.dll dwrite.dll
dxgi.dll dxva2.dll gdi32.dll glu32.dll hid.dll imm32.dll iphlpapi.dll kernel32.dll
kernelbase.dll ksuser.dll mf.dll mfplat.dll mfreadwrite.dll mfuuid.dll mpr.dll
msacm32.dll msvcrt.dll ncrypt.dll netapi32.dll normaliz.dll ntdll.dll ole32.dll
oleacc.dll oleaut32.dll opengl32.dll powrprof.dll propsys.dll psapi.dll rasapi32.dll
rpcrt4.dll sechost.dll secur32.dll setupapi.dll shell32.dll shlwapi.dll srvcli.dll
ucrtbase.dll urlmon.dll user32.dll userenv.dll usp10.dll uxtheme.dll version.dll
win32u.dll windowscodecs.dll winhttp.dll wininet.dll winmm.dll winscard.dll
winspool.drv wintrust.dll wldap32.dll ws2_32.dll wtsapi32.dll
""".split())


def is_platform(name):
    return name in OS_DLLS or name.startswith(("api-ms-win-", "ext-ms-win-"))


def inspect_pe(path):
    # Imported lazily so platform-independent controls need no Windows tooling.
    import pefile
    with pefile.PE(str(path), fast_load=True) as pe:
        if pe.FILE_HEADER.Machine != 0x8664:
            raise ValueError(f"Not an x64 PE binary: {path}")
        pe.parse_data_directories(directories=[
            pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"],
            pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT"],
        ])
        return sorted({entry.dll.decode("ascii").lower()
                       for kind in ("DIRECTORY_ENTRY_IMPORT", "DIRECTORY_ENTRY_DELAY_IMPORT")
                       for entry in getattr(pe, kind, [])})


def resolve_graph(roots, directories, inspect=inspect_pe):
    available = {}
    for directory in directories:
        for path in directory.iterdir():
            if path.is_file() and path.suffix.lower() == ".dll":
                available.setdefault(path.name.lower(), []).append(path.resolve())
    graph = {path.name.lower(): path.resolve() for path in roots}
    queue, imports = list(graph), {}
    while queue:
        name = queue.pop()
        imports[name] = inspect(graph[name])
        for dependency in imports[name]:
            if any(part in dependency for part in ("ggml", "whisper", "msys-2.0", "cygwin")):
                raise ValueError(f"Unexpected runtime dependency: {name} -> {dependency}")
            if is_platform(dependency) or dependency in graph:
                continue
            candidates = set(available.get(dependency, []))
            if len(candidates) != 1:
                raise ValueError(f"Missing or ambiguous DLL: {name} -> {dependency}: {candidates}")
            graph[dependency] = candidates.pop()
            queue.append(dependency)
    return graph, imports


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("visualsubforge", "vsf", "ffmpeg", "ffprobe", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--library-dir", type=Path, action="append", required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    bundle = root / "VisualSubForge"
    bundle.mkdir()
    roots = [getattr(args, name).resolve(strict=True) for name in ("visualsubforge", "vsf", "ffmpeg", "ffprobe")]
    graph, imports = resolve_graph(roots, [p.resolve(strict=True) for p in args.library_dir])
    records = []
    for name, source in sorted(graph.items()):
        target = bundle / name
        shutil.copy2(source, target)
        records.append({"path": name, "source": str(source), "sha256": digest(target),
                        "imports": imports[name],
                        "platform_imports": [item for item in imports[name] if is_platform(item)]})
    # Re-resolve only from the private directory to ensure assembly is complete.
    copied, copied_imports = resolve_graph([bundle / p.name.lower() for p in roots], [bundle])
    if set(copied) != set(graph) or copied_imports != imports:
        raise ValueError("Assembled PE graph changed")
    shutil.copy2(ROOT / "LICENSE", bundle / "LICENSE.txt")
    receipt = {"schema": 1, "architecture": "x86_64", "version": "0.0.1-alpha",
               "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
               "files": records, "ggml_policy": "No Whisper/GGML imports allowed, including delay imports",
               "scope": "Static PE graph only; runtime loading and installation are separate checks",
               "redistribution": "Full source/license inventory still required before release"}
    (bundle / "native-runtime.json").write_text(json.dumps(receipt, indent=2) + "\n")
    # Tauri's NSIS backend installs private resources beside the main binary and
    # installs WebView2 when absent. The GUI is supplied by its normal build path.
    resources = {str(path): path.name for path in bundle.iterdir() if path.name != "visualsubforge.exe"}
    config = {"bundle": {"active": True, "targets": ["nsis"], "resources": resources,
                         "license": "GPL-3.0-or-later", "shortDescription": "Subtitle editor and resync CLI",
                         "windows": {"webviewInstallMode": {"type": "downloadBootstrapper", "silent": True},
                                     "nsis": {"installMode": "currentUser"}}}}
    (root / "tauri.bundle.json").write_text(json.dumps(config, indent=2) + "\n")
    (root / "assembly-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"Assembled {len(records)} native files: {bundle}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Windows packaging failed: {error}")
