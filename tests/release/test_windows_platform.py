"""MSVC linker evidence controls runnable on every development host."""
import importlib.util
from pathlib import Path, PureWindowsPath
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
SPEC = importlib.util.spec_from_file_location('windows_platform', ROOT / 'scripts/windows-platform.py')
PLATFORM = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLATFORM)


class WindowsPlatform(unittest.TestCase):
    def test_default_libraries_and_debug_visualizers_retain_their_paths(self):
        text = ('"/defaultlib:C:\\Program Files\\MSVC\\libcmt.lib"\n'
                '"/natvis:C:\\Rust\\libstd.natvis"\n'
                '"D:\\build\\app.obj"\n'
                '"/DEFAULTLIB:C:\\Program Files\\MSVC\\libcmt.lib"\n')
        self.assertEqual(set(PLATFORM.input_paths(text)), {
            PureWindowsPath('C:/Program Files/MSVC/libcmt.lib'),
            PureWindowsPath('C:/Rust/libstd.natvis'), PureWindowsPath('D:/build/app.obj')})

    def test_unknown_options_and_relative_inputs_are_rejected(self):
        for text in ['', 'relative.lib', 'C:relative.lib', '/unknown:C:\\SDK\\input.lib']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                PLATFORM.input_paths(text)
