#!/usr/bin/env python3
"""Generate the MSVC import library from the DLL that the package will ship."""
import argparse
from pathlib import Path
import shutil
import subprocess
import pefile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--mpv", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
dlls = list((args.mpv / "build").glob("*mpv*.dll"))
if len(dlls) != 1:
    parser.error(f"Expected one built libmpv DLL: {dlls}")
dll = dlls[0]
args.output.mkdir(parents=True, exist_ok=False)
shutil.copytree(args.mpv / "prepared/source/include/mpv", args.output / "include/mpv")
library = args.output / "lib"
library.mkdir()
with pefile.PE(str(dll)) as pe:
    names = sorted(symbol.name.decode("ascii") for symbol in pe.DIRECTORY_ENTRY_EXPORT.symbols
                   if symbol.name and symbol.name.startswith(b"mpv_"))
if not {"mpv_create", "mpv_initialize", "mpv_terminate_destroy"} <= set(names):
    parser.error("Required mpv C API exports are missing")
definition = library / "mpv.def"
definition.write_text(f"LIBRARY {dll.name}\nEXPORTS\n" + "\n".join(names) + "\n")
subprocess.run(["lib.exe", "/nologo", "/machine:x64", f"/def:{definition}",
                f"/out:{library / 'mpv.lib'}"], check=True)
