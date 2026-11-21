"""Exercise real bundle signing with unsigned helpers, as on Intel Homebrew."""
import argparse
import importlib.util
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("package_macos", ROOT / "scripts/package-macos.py")
PACKAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGER)


@unittest.skipUnless(sys.platform == "darwin", "Requires macOS Mach-O and codesign tools")
class MacOSAssembly(unittest.TestCase):
    def test_unsigned_helpers_are_relocated_and_sealed_without_changing_inputs(self):
        with tempfile.TemporaryDirectory(prefix="vsf assembly ") as directory:
            root = Path(directory)
            source = root / "Input.app"
            executables = source / "Contents/MacOS"
            executables.mkdir(parents=True)
            with (source / "Contents/Info.plist").open("wb") as stream:
                plistlib.dump({"CFBundleExecutable": "visualsubforge",
                              "CFBundleIdentifier": "com.golgeek.vsf.assembly-test",
                              "CFBundlePackageType": "APPL",
                              "CFBundleVersion": "1"}, stream)
            library_source = root / "library.c"
            library_source.write_text("int answer(void) { return 42; }\n")
            library = root / "libanswer.dylib"
            subprocess.run(["xcrun", "clang", "-mmacosx-version-min=15.0", "-dynamiclib", str(library_source),
                            "-install_name", str(library), "-o", str(library)], check=True)
            program_source = root / "program.c"
            program_source.write_text('#include <stdio.h>\nextern int answer(void);\n'
                                      'int main(void) { printf("%d\\n", answer()); return 0; }\n')
            main = executables / "visualsubforge"
            subprocess.run(["xcrun", "clang", "-mmacosx-version-min=15.0", str(program_source), str(library),
                            "-Wl,-no_adhoc_codesign", "-o", str(main)], check=True)
            # ARM linkers insert ad-hoc signatures, which masked the failure
            # seen with Intel's unsigned ffmpeg. Link an unsigned fixture on
            # either CPU: stripping with macOS 14 codesign can leave LINKEDIT
            # padding that Xcode 16.2 install_name_tool cannot process.
            unsigned = subprocess.run(["codesign", "--display", str(main)], capture_output=True)
            self.assertNotEqual(unsigned.returncode, 0, "Fixture must be unsigned before assembly")
            tools = {}
            (root / "sdk/bin").mkdir(parents=True)
            for name in ["vsf", "ffmpeg", "ffprobe"]:
                tools[name] = (root if name == "vsf" else root / "sdk/bin") / name
                shutil.copy2(main, tools[name])
            inputs = [main, library, *tools.values(), source / "Contents/Info.plist"]
            before = {path: PACKAGER.digest(path) for path in inputs}
            destination = root / "Relocated App.app"
            PACKAGER.assemble(argparse.Namespace(app=source, output=destination,
                                                 library_dir=[], release_floors=True, ffmpeg_sdk=root / "sdk", **tools))

            subprocess.run(["codesign", "--verify", "--deep", "--strict",
                            str(destination)], check=True)
            for name in ["visualsubforge", *tools]:
                binary = destination / "Contents/MacOS" / name
                self.assertEqual(subprocess.check_output([str(binary)], text=True), "42\n")
                self.assertIn("@loader_path/../Frameworks/libanswer.dylib",
                              PACKAGER.linked_libraries(binary))
            self.assertEqual(before, {path: PACKAGER.digest(path) for path in inputs})
            receipt = json.loads((root / "assembly-receipt.json").read_text())
            for relative, expected in receipt["bundle_files_sha256"].items():
                self.assertEqual(PACKAGER.digest(destination / relative), expected, relative)

            # A developer can inspect their local dependency floor. The exact
            # same dependency graph must fail the hosted release gate.
            subprocess.run(["xcrun", "clang", "-dynamiclib", "-mmacosx-version-min=15.1",
                            str(library_source), "-install_name", str(library), "-o", str(library)], check=True)
            (root / "release-attempt").mkdir()
            with self.assertRaisesRegex(ValueError, "deployment target"):
                PACKAGER.assemble(argparse.Namespace(app=source, output=root / "release-attempt/Test.app",
                    library_dir=[], release_floors=True, ffmpeg_sdk=root / "sdk", **tools))
            (root / "developer").mkdir()
            PACKAGER.assemble(argparse.Namespace(app=source, output=root / "developer/Test.app",
                library_dir=[], release_floors=False, ffmpeg_sdk=root / "sdk", **tools))
            local = json.loads((root / "developer/assembly-receipt.json").read_text())
            self.assertEqual(local["minimum_macos"], "15.1")

            # Test both modes against an actual foreign avcodec load command.
            subprocess.run(["xcrun", "clang", "-dynamiclib", "-mmacosx-version-min=15.0",
                            str(library_source), "-install_name", str(library), "-o", str(library)], check=True)
            foreign = root / "libavcodec.66.dylib"
            shutil.copy2(library, foreign)
            subprocess.run(["install_name_tool", "-change", str(library), str(foreign), str(main)], check=True)
            for release in (False, True):
                with self.assertRaisesRegex(ValueError, "outside the selected SDK"):
                    PACKAGER.assemble(argparse.Namespace(app=source, output=root / "release-attempt/Test.app",
                        library_dir=[], release_floors=release, ffmpeg_sdk=root / "sdk", **tools))


if __name__ == "__main__":
    unittest.main()
