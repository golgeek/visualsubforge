"""Disk image content and Finder geometry contract, independent of hdiutil."""
from pathlib import Path
import os
import runpy
import shutil
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import macos_dmg as dmg


class WindowSettings(unittest.TestCase):
    def test_retina_background_dimensions(self):
        for name, size in (("background.png", (640, 480)), ("background@2x.png", (1280, 960))):
            self.assertEqual(struct.unpack(">II", (dmg.ASSETS / name).read_bytes()[16:24]), size)

    def test_layout(self):
        paths = {"app": "/package/VisualSubForge.app", "icon": str(dmg.ICON), "background": "/tmp/background.tiff"}
        layout = runpy.run_path(str(dmg.ASSETS / "settings.py"), init_globals={"defines": paths})
        self.assertEqual(layout["window_rect"][1], (640, 428))
        self.assertEqual(layout["icon_locations"], {dmg.APP: (176, 186), "Applications": (464, 186)})
        self.assertEqual((layout["icon_size"], layout["text_size"], layout["label_pos"]), (128, 13, "bottom"))
        self.assertEqual(layout["format"], "UDZO")
        self.assertEqual(layout["files"], [paths["app"]])
        self.assertEqual(layout["symlinks"], {"Applications": "/Applications"})
        self.assertEqual(layout["background"], paths["background"])
        self.assertEqual(layout["icon"], paths["icon"])
        self.assertEqual(layout["default_view"], "icon-view")
        self.assertIsNone(layout["arrange_by"])
        for key in ("show_toolbar", "show_sidebar", "show_pathbar", "show_status_bar", "show_tab_view", "show_icon_preview"):
            self.assertFalse(layout[key])


@unittest.skipIf(os.name == "nt", "Volume fixture uses POSIX symbolic links")
class ImageContents(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.volume = self.root / "mount"
        self.volume.mkdir()
        self.app = self.root / dmg.APP
        self.app.mkdir()
        (self.app / "executable").write_bytes(b"app fixture")
        self.copy = self.volume / dmg.APP
        shutil.copytree(self.app, self.copy)
        self.link = self.volume / "Applications"
        self.link.symlink_to("/Applications")
        self.asset = self.root / "expected"
        self.asset.write_bytes(b"asset fixture")
        for name in (".DS_Store", ".background.tiff", ".VolumeIcon.icns"):
            shutil.copyfile(self.asset, self.volume / name)

    def check(self):
        return dmg.check_contents(self.volume, self.app, self.asset, self.asset)

    def test_complete(self):
        self.assertEqual(len(self.check()), 64)

    def test_extra_entry(self):
        for name in ("README", ".extra"):
            with self.subTest(name=name):
                path = self.volume / name
                path.touch()
                with self.assertRaises(ValueError):
                    self.check()
                path.unlink()

    def test_missing_app(self):
        shutil.rmtree(self.copy)
        with self.assertRaises(ValueError):
            self.check()

    def test_wrong_shortcut(self):
        self.link.unlink()
        self.link.symlink_to("/tmp")
        with self.assertRaises(ValueError):
            self.check()

    def test_links_cannot_escape_even_if_both_trees_agree(self):
        for app in (self.app, self.copy):
            (app / "alias").symlink_to(self.asset)
        with self.assertRaises(ValueError):
            self.check()

    def test_changed_assets(self):
        for name in (".background.tiff", ".VolumeIcon.icns"):
            with self.subTest(name=name):
                (self.volume / name).write_bytes(b"different")
                with self.assertRaises(ValueError):
                    self.check()
                shutil.copyfile(self.asset, self.volume / name)

    def test_changed_app_file(self):
        (self.copy / "executable").write_bytes(b"different")
        with self.assertRaises(ValueError):
            self.check()
