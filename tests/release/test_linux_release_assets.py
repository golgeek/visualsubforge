"""Both native Linux builds contribute their own packages and source archives."""
import hashlib
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[2] / "scripts"
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("release_assets", SCRIPTS / "release-assets.py")
ASSETS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ASSETS)


class LinuxReleaseAssets(unittest.TestCase):
    def test_both_architectures_and_source_archives_survive_collection(self):
        for signed in (False, True):
            with self.subTest(signed=signed):
                self.check_collection(signed)

    def check_collection(self, signed):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            inputs, output = root / "inputs", root / "downloads"

            def artifact(name, files):
                directory = inputs / name
                directory.mkdir(parents=True)
                manifest = []
                for file in files:
                    data = f"{name}/{file}".encode()
                    (directory / file).write_bytes(data)
                    manifest.append(f"{hashlib.sha256(data).hexdigest()}  {file}\n")
                (directory / "SHA256SUMS").write_text("".join(manifest))

            sources = ["native-sources.tar.gz", "application-sources.tar.gz"]
            artifact("windows-preview-x64", ["Windows-setup.exe", "VisualSubForge_0.1.0_x64_windows.zip", *sources])
            expected = {}
            for arch, deb in [("x86_64", "amd64"), ("aarch64", "arm64")]:
                base = f"VisualSubForge_0.1.0_{arch}_macos"
                artifact("macos-unsigned-" + arch, [base + "-unsigned.zip", base + "-unsigned.dmg", *sources])
                artifact("macos-notarized-" + arch, [base + ".dmg"])
                image = base + (".dmg" if signed else "-unsigned.dmg")
                source = ("macos-notarized-" if signed else "macos-unsigned-") + arch
                expected[image] = f"{source}/{image}".encode()
                name = "linux-preview-" + deb
                packages = [f"VisualSubForge_0.1.0_{arch}_linux.tar.gz", f"VisualSubForge_0.1.0_{deb}.deb"]
                artifact(name, packages + sources)
                for file in packages:
                    expected[file] = f"{name}/{file}".encode()
                for file in sources:
                    expected[f"VisualSubForge_0.1.0_linux-{deb}_{file}"] = f"{name}/{file}".encode()
            artifact("preview-acceptance-fixtures", ["synthetic.srt"])
            with patch.object(ASSETS, "check", return_value="0.1.0"):
                ASSETS.collect(inputs, output, signed=signed)
            verified = ASSETS.verified(output)
            self.assertEqual({n for n in verified if "_macos" in n and not n.endswith(".tar.gz")},
                             {n for n in expected if n.endswith(".dmg")})
            for name, data in expected.items():
                self.assertEqual(verified[name].read_bytes(), data)
