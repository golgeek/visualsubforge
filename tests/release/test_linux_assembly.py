"""Real ELF relocation control; source licensing/install tests are separate."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location("package_linux", ROOT / "scripts/package-linux.py")
PACKAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGER)


class LddGraph(unittest.TestCase):
    def test_loader_without_arrow_is_part_of_the_graph(self):
        actual = PACKAGER.parse_ldd("""
        linux-vdso.so.1 (0x00007ffe8cdaa000)
        libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007f18800d0000)
        /lib64/ld-linux-x86-64.so.2 (0x00007f18802e0000)
        """)
        self.assertEqual(actual, {
            "libc.so.6": Path("/lib/x86_64-linux-gnu/libc.so.6"),
            "ld-linux-x86-64.so.2": Path("/lib64/ld-linux-x86-64.so.2"),
        })
        with self.assertRaisesRegex(ValueError, "Unresolved ELF"):
            PACKAGER.parse_ldd("libmedia.so => not found")


@unittest.skipUnless(sys.platform == "linux", "Requires Linux ELF and Debian package tools")
class LinuxAssembly(unittest.TestCase):
    def test_private_library_survives_sdk_removal_and_inputs_stay_unchanged(self):
        with tempfile.TemporaryDirectory(prefix="vsf ELF assembly ") as directory:
            root = Path(directory)
            sdk = root / "sdk"
            sdk.mkdir()
            (sdk / "answer.c").write_text("int answer(void) { return 42; }\n")
            subprocess.run(["cc", "-shared", "-fPIC", str(sdk / "answer.c"), "-Wl,-soname,libanswer.so.1",
                            "-o", str(sdk / "libanswer.so.1")], check=True)
            (sdk / "main.c").write_text('#include <stdio.h>\nextern int answer(void);\n'
                                       'int main(void) { printf("%d\\n", answer()); return 0; }\n')
            main = sdk / "visualsubforge"
            subprocess.run(["cc", str(sdk / "main.c"), str(sdk / "libanswer.so.1"),
                            f"-Wl,-rpath,{sdk}", "-o", str(main)], check=True)
            binaries = {"visualsubforge": main}
            for name in ("vsf", "ffmpeg", "ffprobe"):
                binaries[name] = sdk / name
                shutil.copy2(main, binaries[name])
            originals = {path: PACKAGER.digest(path) for path in sdk.iterdir()}
            platform = set(PACKAGER.dependencies(Path("/bin/true"), os.environ).values())
            args = argparse.Namespace(**binaries, output=root / "output", library_dir=[sdk],
                                      native_source=[], native_notices=[])
            # This fixture has no distro media libraries. Keep source download
            # checks in the real hosted package, not in a network-dependent unit test.
            def no_sources(packages, destination, notices):
                self.assertEqual(packages, {})
                destination.mkdir()
            with patch.object(PACKAGER, "platform_runtime", return_value=(platform, {})), \
                    patch.object(PACKAGER, "collect_distro_sources", side_effect=no_sources):
                PACKAGER.assemble(args)
            self.assertEqual(originals, {path: PACKAGER.digest(path) for path in originals})
            sdk.rename(root / "hidden-sdk")
            private = args.output / "root/usr/lib/visualsubforge"
            env = {key: value for key, value in os.environ.items() if not key.startswith("LD_")}
            result = subprocess.check_output([str(private / "vsf")], env=env, text=True)
            self.assertEqual(result, "42\n")
            receipt = json.loads((args.output / "assembly-receipt.json").read_text())
            for item in receipt["files"]:
                self.assertEqual(PACKAGER.digest(private / item["path"]), item["sha256"])
            self.assertEqual(PACKAGER.digest(args.output / receipt["artifact"]["file"]), receipt["artifact"]["sha256"])
