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
import tarfile
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


class RuntimeProviders(unittest.TestCase):
    def test_partial_or_duplicate_package_mappings_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            mapping = Path(directory) / "providers.tsv"
            row = "libasound.so.2\tlibasound2\tlibasound2t64\talsa-lib\talsa-lib"
            for invalid in (row.rsplit("\t", 1)[0], row + "\n" + row):
                with self.subTest(invalid=invalid):
                    mapping.write_text(invalid + "\n")
                    with self.assertRaises(ValueError):
                        PACKAGER.read_system_providers(mapping)

    def test_direct_system_edge_cannot_be_omitted(self):
        files = [{"path": "bin/vsf", "needed": ["libasound.so.2", "libmpv.so.2"]},
                 {"path": "lib/libmpv.so.2", "needed": ["libc.so.6"]}]
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "libasound"):
                PACKAGER.write_runtime_packages(Path(directory) / "providers.tsv", files, {"libc.so.6": "/lib/libc.so.6"})

    def test_launcher_passes_every_checked_soname_and_preserves_app_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            doc = root / "share/doc/visualsubforge"
            doc.mkdir(parents=True)
            names = sorted(PACKAGER.SYSTEM_PROVIDERS)
            (doc / "runtime-packages.tsv").write_text("".join(
                "\t".join([name, *PACKAGER.SYSTEM_PROVIDERS[name]]) + "\n" for name in names))
            binaries = root / "bin"
            binaries.mkdir()
            shutil.copy2(ROOT / "scripts/linux-portable/visualsubforge", binaries / "visualsubforge")
            for program, output in (("check-runtime", "checked"), ("visualsubforge.real", "opened")):
                script = binaries / program
                script.write_text(f'#!/bin/sh\nprintf "%s\\n" "$@" > {output}\n')
                script.chmod(0o755)
            subprocess.run([binaries / "visualsubforge", "with spaces", "*.srt"], cwd=root, check=True)
            self.assertEqual((root / "checked").read_text().splitlines(), names)
            self.assertEqual((root / "opened").read_text().splitlines(), ["with spaces", "*.srt"])


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
            # Even a resolved dependency must have a reviewed provider. Exercise
            # both failure paths with the same real SDK library and RUNPATH.
            for name, system, search, error in (
                ("unreviewed", platform, [], "Unreviewed system library"),
                ("overlap", platform | {root / "system/libanswer.so.2"}, [sdk], "overlaps system runtime"),
            ):
                with self.subTest(name=name), patch.object(PACKAGER, "platform_runtime", return_value=system):
                    trial = argparse.Namespace(**{**vars(args), "library_dir": search, "output": root / name})
                    with self.assertRaisesRegex(ValueError, error):
                        PACKAGER.assemble(trial)
            with patch.object(PACKAGER, "platform_runtime", return_value=platform), \
                    patch.object(PACKAGER, "collect_distro_sources", side_effect=no_sources):
                PACKAGER.assemble(args)
            self.assertEqual(originals, {path: PACKAGER.digest(path) for path in originals})
            sdk.rename(root / "hidden-sdk")
            private = args.output / "root/usr/lib/visualsubforge"
            env = {key: value for key, value in os.environ.items() if not key.startswith("LD_")}
            result = subprocess.check_output([str(private / "bin/vsf")], env=env, text=True)
            self.assertEqual(result, "42\n")
            receipt = json.loads((args.output / "assembly-receipt.json").read_text())
            self.assertIn("libc6 (>= 2.35)", receipt["platform_dependencies"])
            for item in receipt["files"]:
                self.assertEqual(PACKAGER.digest(private / item["path"]), item["sha256"])
            self.assertEqual(PACKAGER.digest(args.output / receipt["artifact"]["file"]), receipt["artifact"]["sha256"])
            extracted = root / "tar"
            with tarfile.open(args.output / receipt["tarball"]["file"]) as tar:
                tar.extractall(extracted, filter="data")
            distribution, = extracted.iterdir()
            payload = {p.relative_to(private): p.read_bytes() for p in private.rglob("*") if p.is_file()}
            self.assertEqual(payload, {p.relative_to(distribution): p.read_bytes()
                                       for p in distribution.rglob("*") if p.is_file()})
            home = root / "home with spaces"
            home.mkdir()
            installed = home / ".local/opt/visualsubforge"
            install_env = {**env, "HOME": str(home)}
            for _ in range(2):
                subprocess.run([distribution / "install.sh"], env=install_env, check=True)
                self.assertEqual((home / ".local/bin/vsf").resolve(), installed / "bin/vsf")
            (installed / "my file").touch()
            blocked = subprocess.run([distribution / "uninstall.sh"], env=install_env, capture_output=True)
            self.assertNotEqual(blocked.returncode, 0)
            self.assertTrue((installed / "my file").exists())
            (installed / "my file").unlink()
            subprocess.run([installed / "uninstall.sh"], env=install_env, check=True)
            self.assertFalse(installed.exists())
            self.assertEqual(list(home.iterdir()), [])
            unrelated = home / ".local/bin/visualsubforge"
            unrelated.parent.mkdir(parents=True)
            unrelated.write_text("existing program")
            blocked = subprocess.run([distribution / "install.sh"], env=install_env, capture_output=True)
            self.assertNotEqual(blocked.returncode, 0)
            self.assertEqual(unrelated.read_text(), "existing program")
            self.assertFalse(installed.exists())
