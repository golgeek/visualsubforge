"""Deployment-target metadata controls the minimum OS advertised by a package."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("package_macos", ROOT / "scripts/package-macos.py")
PACKAGER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGER)


class MacOSDeploymentTargets(unittest.TestCase):
    def test_current_and_legacy_macos_load_commands(self):
        for command, expected in [
            ("cmd LC_BUILD_VERSION\n platform 1\n minos 14.0\n sdk 15.0\n", "14.0"),
            ("cmd LC_VERSION_MIN_MACOSX\n version 10.13\n sdk 10.15\n", "10.13"),
        ]:
            with self.subTest(command=command), patch.object(
                PACKAGER, "output", return_value="Load command 0\n" + command
            ) as output:
                self.assertEqual(PACKAGER.minimum_macos(Path("library"), "arm64"), expected)
                output.assert_called_once_with("otool", "-arch", "arm64", "-l", Path("library"))

    def test_missing_or_non_macos_target_cannot_lower_the_advertised_minimum(self):
        for command in [
            "cmd LC_BUILD_VERSION\n platform 6\n minos 14.0\n",
            "cmd LC_BUILD_VERSION\n platform 1\n sdk 15.0\n",
            "cmd LC_UUID\n uuid example\n",
        ]:
            with self.subTest(command=command), patch.object(
                PACKAGER, "output", return_value="Load command 0\n" + command
            ):
                with self.assertRaises(ValueError):
                    PACKAGER.minimum_macos(Path("library"), "x86_64")


if __name__ == "__main__":
    unittest.main()
