"""Build the release disk image and compare its mounted payload to the input app."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile

ASSETS = Path(__file__).resolve().parents[1] / "src-tauri/dmg"
ICON = ASSETS.parent / "icons/icon.icns"
VOLUME = "VisualSubForge"
APP = VOLUME + ".app"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def tree(root):
    if root.is_symlink() or not root.is_dir():
        raise ValueError(f"Expected application directory: {root}")
    entries = {}
    for path in sorted(root.rglob("*")):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            target = os.readlink(path)
            if os.path.isabs(target) or not path.resolve(strict=True).is_relative_to(root.resolve()):
                raise ValueError(f"Non-relocatable app link: {name}")
            entries[name] = ["link", target]
        elif path.is_dir():
            entries[name] = ["dir"]
        elif path.is_file():
            entries[name] = ["file", digest(path)]
        else:
            raise ValueError(f"Unexpected app entry: {name}")
    return entries


def check_contents(volume, app, background, icon):
    entries = {p.name for p in volume.iterdir()}
    required = {APP, "Applications", ".DS_Store", ".background.tiff", ".VolumeIcon.icns"}
    # HFS+ volumes can acquire the filesystem event journal on attachment.
    optional = {".fseventsd"}
    if required - entries or entries - required - optional:
        raise ValueError(f"Unexpected volume contents: {sorted(entries)}")
    journal = volume / ".fseventsd"
    if journal.name in entries and (journal.is_symlink() or not journal.is_dir()):
        raise ValueError("Unexpected filesystem journal")
    shortcut = volume / "Applications"
    if not shortcut.is_symlink() or os.readlink(shortcut) != "/Applications":
        raise ValueError("Incorrect Applications shortcut")
    for name in (".DS_Store", ".background.tiff", ".VolumeIcon.icns"):
        path = volume / name
        if path.is_symlink() or not path.is_file() or not path.stat().st_size:
            raise ValueError(f"Expected regular file: {name}")
    for name, source in ((".background.tiff", background), (".VolumeIcon.icns", icon)):
        if digest(volume / name) != digest(source):
            raise ValueError(f"Image asset differs: {name}")
    mounted = tree(volume / APP)
    if mounted != tree(app):
        raise ValueError("Mounted app differs from the input app")
    return hashlib.sha256(json.dumps(mounted, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def make_background(directory):
    background = directory / "background.tiff"
    subprocess.run(["tiffutil", "-cathidpicheck", ASSETS / "background.png",
                    ASSETS / "background@2x.png", "-out", background], check=True)
    return background


def build(image, app):
    import dmgbuild  # Only the macOS builder needs the pinned tools.
    if app.name != APP or not app.is_dir() or image.exists():
        raise ValueError(f"Expected {APP} and a new image path")
    with tempfile.TemporaryDirectory() as tmp:
        background = make_background(Path(tmp))
        dmgbuild.build_dmg(str(image), VOLUME, str(ASSETS / "settings.py"),
                           defines={"app": str(app.resolve()), "icon": str(ICON),
                                    "background": str(background)})


def verify(image, app, signed=False):
    subprocess.run(["hdiutil", "verify", image], check=True)
    info = plistlib.loads(subprocess.check_output(["hdiutil", "imageinfo", "-plist", image]))
    if info["Format"] != "UDZO":
        raise ValueError("Expected compressed read-only UDZO")
    with tempfile.TemporaryDirectory() as tmp:
        volume = Path(tmp) / VOLUME
        background = make_background(Path(tmp))
        volume.mkdir()
        subprocess.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen",
                        "-mountpoint", volume, image], check=True)
        try:
            disk = plistlib.loads(subprocess.check_output(["diskutil", "info", "-plist", volume]))
            if disk["VolumeName"] != VOLUME or disk["WritableVolume"]:
                raise ValueError("Unexpected volume name or writable mount")
            result = check_contents(volume, app, background, ICON)
            if signed:
                for command in (["codesign", "--verify", "--deep", "--strict"],
                                ["xcrun", "stapler", "validate"], ["spctl", "--assess", "--type", "execute"]):
                    subprocess.run([*command, volume / APP], check=True)
            return result
        finally:
            subprocess.run(["hdiutil", "detach", volume], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("build")
    verification = commands.add_parser("verify")
    verification.add_argument("--signed", action="store_true")
    verification.add_argument("--tree-hash-file", type=Path)
    for command in commands.choices.values():
        command.add_argument("app", type=Path)
        command.add_argument("image", type=Path)
    args = parser.parse_args()
    if args.command == "build":
        build(args.image, args.app)
    else:
        tree_hash = verify(args.image, args.app, args.signed)
        if args.tree_hash_file:
            args.tree_hash_file.write_text(tree_hash + "\n")
        print(f"Verified {args.image.name}, app tree SHA-256 {tree_hash}")
