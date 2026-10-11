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
            subprocess.run(["xcrun", "clang", "-dynamiclib", str(library_source),
                            "-install_name", str(library), "-o", str(library)], check=True)
            program_source = root / "program.c"
            program_source.write_text('#include <stdio.h>\nextern int answer(void);\n'
                                      'int main(void) { printf("%d\\n", answer()); return 0; }\n')
            main = executables / "visualsubforge"
            subprocess.run(["xcrun", "clang", str(program_source), str(library),
                            "-o", str(main)], check=True)
            # ARM linkers insert ad-hoc signatures, which masked the failure
            # seen with Intel's unsigned ffmpeg. Force that input on either CPU.
            subprocess.run(["codesign", "--remove-signature", str(main)], check=True)
            tools = {}
            for name in ["vsf", "ffmpeg", "ffprobe"]:
                tools[name] = root / name
                shutil.copy2(main, tools[name])
            inputs = [main, library, *tools.values(), source / "Contents/Info.plist"]
            before = {path: PACKAGER.digest(path) for path in inputs}
            destination = root / "Relocated App.app"
            PACKAGER.assemble(argparse.Namespace(app=source, output=destination,
                                                 library_dir=[], **tools))

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


if __name__ == "__main__":
    unittest.main()
