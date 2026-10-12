#!/usr/bin/env python3
"""Assemble a private macOS media runtime; signing/notarization is a separate step.

This copies inputs without changing them, resolves every linked non-system
library, rejects ambiguous SDKs, and replaces load paths inside the copy. It
does not establish redistribution rights or clean-machine/device acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess


def output(*args):
    return subprocess.check_output(list(map(str, args)), text=True).strip()


def system_library(name):
    return name.startswith(("/System/Library/", "/usr/lib/"))


def linked_libraries(path):
    return [line.strip().split(" (compatibility", 1)[0]
            for line in output("otool", "-L", path).splitlines()[1:]]


def install_id(path):
    lines = output("otool", "-D", path).splitlines()
    return lines[1] if len(lines) > 1 else None


def rpaths(path):
    return re.findall(r"cmd LC_RPATH\s+cmdsize \d+\s+path (.+?) \(offset",
                      output("otool", "-l", path))


def resolve_library(name, owner, executable, directories):
    if name.startswith("/"):
        return Path(name).resolve(strict=True)
    if name.startswith("@loader_path/"):
        candidates = [owner.parent / name.removeprefix("@loader_path/")]
    elif name.startswith("@executable_path/"):
        candidates = [executable.parent / name.removeprefix("@executable_path/")]
    elif name.startswith("@rpath/") or name.startswith("./"):
        relative = name.split("/", 1)[1]
        candidates = [directory / relative for directory in directories]
        for entry in rpaths(owner):
            entry = entry.replace("@loader_path", str(owner.parent))
            entry = entry.replace("@executable_path", str(executable.parent))
            if entry.startswith("/"):
                candidates.append(Path(entry) / relative)
    else:
        raise ValueError(f"Unsupported library reference in {owner}: {name}")
    matches = {candidate.resolve() for candidate in candidates if candidate.is_file()}
    if len(matches) != 1:
        raise ValueError(f"Expected one SDK library for {owner}: {name}; found {sorted(map(str, matches))}")
    return matches.pop()


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def minimum_macos(path, architecture):
    commands = output("otool", "-arch", architecture, "-l", path)
    for command in commands.split("Load command ")[1:]:
        if re.search(r"cmd LC_BUILD_VERSION\b", command):
            # Mach-O PLATFORM_MACOS is 1. Catalyst and simulator targets cannot
            # stand in for a native desktop dependency of the same CPU type.
            if not re.search(r"platform (?:1|MACOS|macos)\s", command):
                raise ValueError(f"Non-macOS build target: {path}")
            match = re.search(r"minos ([\d.]+)", command)
        elif re.search(r"cmd LC_VERSION_MIN_MACOSX\b", command):
            match = re.search(r"version ([\d.]+)", command)
        else:
            continue
        if match:
            return match[1]
    raise ValueError(f"Missing macOS deployment target: {path}")


def assemble(args):
    source = args.app.resolve(strict=True)
    destination = args.output.resolve()
    if destination.exists() or destination.is_relative_to(source):
        raise ValueError("Choose a new output path outside the source app")
    receipt_path = destination.parent / "assembly-receipt.json"
    if receipt_path.exists():
        raise ValueError("The output directory already contains an assembly receipt")
    with (source / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    main = source / "Contents/MacOS" / info["CFBundleExecutable"]
    roots = {main: Path("Contents/MacOS") / main.name}
    for name in ["vsf", "ffmpeg", "ffprobe"]:
        path = getattr(args, name).resolve(strict=True)
        roots[path] = Path("Contents/MacOS") / name
    directories = [path.resolve(strict=True) for path in args.library_dir]
    architecture = output("lipo", "-archs", main).split()
    if len(architecture) != 1:
        raise ValueError("This preview packager expects one architecture per artifact")
    edges, destinations, ids, targets = {}, dict(roots), {}, {}
    minimums = [info.get("LSMinimumSystemVersion", "11.0")]
    queue = list(roots)
    while queue:
        original = queue.pop()
        if architecture[0] not in output("lipo", "-archs", original).split():
            raise ValueError(f"Architecture mismatch: {original}")
        targets[original] = minimum_macos(original, architecture[0])
        minimums.append(targets[original])
        ids[original] = install_id(original)
        edges[original] = {}
        for name in linked_libraries(original):
            if system_library(name) or name == ids[original]:
                continue
            library = resolve_library(name, original, main, directories)
            edges[original][name] = library
            if library not in destinations:
                relative = Path("Contents/Frameworks") / library.name
                if relative in destinations.values():
                    raise ValueError(f"Conflicting library basename: {library.name}")
                destinations[library] = relative
                queue.append(library)
    # All input validation above is read-only. A failed assembly leaves its new
    # output for diagnosis; never replace a previous artifact or a source SDK.
    shutil.copytree(source, destination)
    for original, relative in destinations.items():
        copied = destination / relative
        copied.parent.mkdir(parents=True, exist_ok=True)
        if original != main:
            shutil.copy2(original, copied)
        copied.chmod(0o755)
    records = []
    for original, relative in destinations.items():
        copied = destination / relative
        for name, dependency in edges[original].items():
            replacement = "@loader_path/" + os.path.relpath(
                destination / destinations[dependency], copied.parent)
            subprocess.run(["install_name_tool", "-change", name, replacement, str(copied)], check=True)
        if ids[original]:
            subprocess.run(["install_name_tool", "-id", "@rpath/" + copied.name, str(copied)], check=True)
        for entry in set(rpaths(copied)):
            subprocess.run(["install_name_tool", "-delete_rpath", entry, str(copied)], check=True)
        # Every linked dependency must now be either an OS library or a real
        # file inside this app. No developer-machine path may survive.
        for name in linked_libraries(copied):
            if system_library(name) or name == install_id(copied):
                continue
            if not name.startswith("@loader_path/"):
                raise ValueError(f"Unrelocated library in {copied}: {name}")
            dependency = (copied.parent / name.removeprefix("@loader_path/")).resolve(strict=True)
            if not dependency.is_relative_to(destination):
                raise ValueError(f"Dependency escapes app: {dependency}")
        records.append({"file": str(relative), "source": str(original),
                        "minimum_macos": targets[original],
                        "source_sha256": digest(original)})
    version = json.loads((Path(__file__).resolve().parent.parent / "package.json").read_text())["version"]
    numeric_version = version.split("-", 1)[0]
    info.update(CFBundleShortVersionString=numeric_version, CFBundleVersion=numeric_version,
                VSFReleaseVersion=version,
                LSMinimumSystemVersion=max(minimums, key=lambda value: tuple(map(int, value.split(".")))))
    with (destination / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump(info, stream)
    resources = destination / "Contents/Resources"
    resources.mkdir(exist_ok=True)
    shutil.copy2(Path(__file__).resolve().parent.parent / "LICENSE", resources / "LICENSE")
    application_inventory = None
    if getattr(args, "application_sources", None):
        application = args.application_sources.resolve(strict=True)
        application_inventory = digest(application / "application-inventory.json")
        shutil.copytree(application / "notices", resources / "third-party/application")
        shutil.copy2(application / "application-inventory.json", resources / "application-inventory.json")
    manifest = {"schema": 1, "version": version, "architecture": architecture[0],
                "minimum_macos": info["LSMinimumSystemVersion"],
                "signature": "ad-hoc; Developer ID signing and notarization pending",
                "redistribution_inventory": "pending", "files": records,
                "application_inventory_sha256": application_inventory}
    (resources / "native-runtime.json").write_text(json.dumps(manifest, indent=2) + "\n")
    # Sign nested code before the app. codesign treats even the main executable
    # path as a bundle operation and rejects unsigned helpers (common on Intel).
    # The outer signature seals the main executable and all finished resources.
    for original, relative in destinations.items():
        if original != main:
            subprocess.run(["codesign", "--force", "--sign", "-",
                            str(destination / relative)], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(destination)], check=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(destination)], check=True)
    # Signing the outer bundle changes its main executable. Final hashes must
    # live outside the app: embedding its own final hash creates a cycle through
    # the CodeResources signature. The receipt covers web assets/resources too.
    receipt = dict(manifest)
    receipt["files"] = [dict(record, bundled_sha256=digest(destination / record["file"]))
                        for record in records]
    receipt["bundle_files_sha256"] = {
        str(path.relative_to(destination)): digest(path)
        for path in sorted(destination.rglob("*")) if path.is_file() and not path.is_symlink()
    }
    receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"app": str(destination), "native_files": len(records),
                      "minimum_macos": info["LSMinimumSystemVersion"]}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--vsf", type=Path, required=True)
    parser.add_argument("--ffmpeg", type=Path, required=True)
    parser.add_argument("--ffprobe", type=Path, required=True)
    parser.add_argument("--library-dir", type=Path, action="append", default=[])
    parser.add_argument("--application-sources", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        assemble(args)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging failed: {error}\n")


if __name__ == "__main__":
    main()
