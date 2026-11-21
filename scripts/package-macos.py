#!/usr/bin/env python3
"""Assemble a private macOS media runtime; signing/notarization is a separate step.

This copies inputs without changing them, resolves every linked non-system
library, rejects ambiguous SDKs, and replaces load paths inside the copy. It
does not establish redistribution rights or clean-machine/device acceptance.
"""
from release_version import release_version, source_inventory_reference

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


def minimum_macos(path, architecture, *, require_build_version=True):
    commands = output("otool", "-arch", architecture, "-l", path)
    for command in commands.split("Load command ")[1:]:
        if re.search(r"cmd LC_BUILD_VERSION\b", command):
            # Mach-O PLATFORM_MACOS is 1. Catalyst and simulator targets cannot
            # stand in for a native desktop dependency of the same CPU type.
            if not re.search(r"platform (?:1|MACOS|macos)\s", command):
                raise ValueError(f"Non-macOS build target: {path}")
            match = re.search(r"minos ([\d.]+)", command)
        elif not require_build_version and re.search(r"cmd LC_VERSION_MIN_MACOSX\b", command):
            match = re.search(r"version ([\d.]+)", command)
        else:
            continue
        if match:
            return match[1]
    raise ValueError(f"Missing macOS deployment target: {path}")


FFMPEG_LIBRARY = re.compile(r"lib(?:av(?:codec|device|filter|format|util|resample)|sw(?:resample|scale)|postproc)(?:\.[0-9]+)*\.dylib$")
MACHO_MAGICS = {bytes.fromhex(value) for value in (
    "feedface", "cefaedfe", "feedfacf", "cffaedfe", "cafebabe", "bebafeca", "cafebabf", "bfbafeca")}


def version_number(value):
    parts = tuple(map(int, value.split(".")))
    return parts + (0,) * (3 - len(parts))


def verify_source_tree(source, roots):
    # Tauri may start copying additional helpers or frameworks in the future.
    # All executable images must participate in the same dependency and floor audit.
    for path in source.rglob("*"):
        if path.is_file() and path.resolve() not in roots:
            with path.open("rb") as stream:
                if stream.read(4) in MACHO_MAGICS:
                    raise ValueError(f"Unaccounted Mach-O in input application: {path}")


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
    ffmpeg_sdk = args.ffmpeg_sdk.resolve(strict=True)
    for name in ("ffmpeg", "ffprobe"):
        if (getattr(args, name).resolve(strict=True).parent != ffmpeg_sdk / "bin"
                or getattr(args, name).name != name):
            raise ValueError(f"{name} is outside the selected FFmpeg SDK")
    verify_source_tree(source, roots)
    directories = [path.resolve(strict=True) for path in args.library_dir]
    architecture = output("lipo", "-archs", main).split()
    if len(architecture) != 1 or architecture[0] not in ("arm64", "x86_64"):
        raise ValueError("The macOS packager expects one arm64 or x86_64 architecture per artifact")
    edges, destinations, ids, targets = {}, dict(roots), {}, {}
    minimums = ["15.0", info.get("LSMinimumSystemVersion", "15.0")]
    ffmpeg_links = []
    slice_targets = {}
    if args.release_floors and version_number(minimums[-1]) > (15, 0, 0):
        raise ValueError(f"Input app requires macOS {minimums[-1]}")
    queue = list(roots)
    while queue:
        original = queue.pop()
        slices = output("lipo", "-archs", original).split()
        if architecture[0] not in slices:
            raise ValueError(f"Architecture mismatch: {original}")
        built_here = (original in roots or original.is_relative_to(ffmpeg_sdk)
                      or original.name.startswith(("libmpv.", "libplacebo.")))
        deployment = {cpu: minimum_macos(original, cpu, require_build_version=args.release_floors)
                      for cpu in slices}
        slice_targets[original] = deployment
        targets[original] = deployment[architecture[0]]
        minimums.extend(deployment.values())
        for cpu, floor in deployment.items():
            if args.release_floors and (version_number(floor) > (15, 0, 0)
                                        or (built_here and version_number(floor) != (15, 0, 0))):
                raise ValueError(f"Invalid macOS release deployment target {floor} for {cpu}: {original}")
        ids[original] = install_id(original)
        edges[original] = {}
        for name in linked_libraries(original):
            av_library = FFMPEG_LIBRARY.fullmatch(Path(name).name)
            if av_library and system_library(name):
                raise ValueError(f"FFmpeg dependency outside the selected SDK: {original} -> {name}")
            if system_library(name) or name == ids[original]:
                continue
            library = resolve_library(name, original, main, directories)
            if re.search(r"/Cellar/ffmpeg(?:@[^/]+)?/", library.as_posix()):
                raise ValueError(f"Homebrew FFmpeg dependency: {original} -> {name} -> {library}")
            if av_library or FFMPEG_LIBRARY.fullmatch(library.name):
                if not library.is_relative_to(ffmpeg_sdk / "lib"):
                    raise ValueError(f"FFmpeg dependency outside the selected SDK: {original} -> {name} -> {library}")
                ffmpeg_links.append({"file": str(destinations[original]), "load_command": name,
                                     "resolved_source": str(library),
                                     "bundled_target": "Contents/Frameworks/" + library.name})
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
                        "slice_minimum_macos": slice_targets[original],
                        "minimum_macos": targets[original],
                        "source_sha256": digest(original)})
    for link in ffmpeg_links:
        consumer = destination / link["file"]
        dependency = destination / link["bundled_target"]
        relocated = "@loader_path/" + os.path.relpath(dependency, consumer.parent)
        if relocated not in linked_libraries(consumer) or not dependency.is_file():
            raise ValueError(f"Incorrect packaged FFmpeg link: {consumer} -> {relocated}")
        link["bundled_load_command"] = relocated
    version = release_version()
    numeric_version = re.split("[-+]", version, maxsplit=1)[0]
    info.update(CFBundleShortVersionString=numeric_version, CFBundleVersion=numeric_version,
                VSFReleaseVersion=version,
                LSMinimumSystemVersion=max(minimums, key=version_number))
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
    native_inventory = None
    if getattr(args, "native_sources", None):
        from macos_sources import collect
        native_inventory = collect(records, args.homebrew_cellar, args.mpv_build, args.angle_sdk,
                                   args.angle_receipt, args.native_sources, resources,
                                   placebo=getattr(args, "placebo_build", None), ffmpeg=ffmpeg_sdk.parent)
    target = "macos-aarch64" if architecture[0] == "arm64" else "macos-x86_64"
    manifest = {"schema": 1, "version": version, "architecture": architecture[0],
                "minimum_macos": info["LSMinimumSystemVersion"], "ffmpeg_links": ffmpeg_links,
                "signature": "Assembly used an ad-hoc signature. The release signing workflow records Developer ID signing and notarization.",
                "redistribution_inventory": source_inventory_reference(version, target), "files": records,
                "application_inventory_sha256": application_inventory,
                "native_inventory_sha256": native_inventory}
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
    parser.add_argument("--release-floors", action="store_true", help="Require compatibility with macOS 15.0")
    parser.add_argument("--ffmpeg-sdk", type=Path, required=True)
    parser.add_argument("--ffmpeg", type=Path, required=True)
    parser.add_argument("--ffprobe", type=Path, required=True)
    parser.add_argument("--library-dir", type=Path, action="append", default=[])
    parser.add_argument("--application-sources", type=Path)
    parser.add_argument("--placebo-build", type=Path)
    for name in ["native-sources", "homebrew-cellar", "mpv-build", "angle-sdk", "angle-receipt"]:
        parser.add_argument("--" + name, type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    inventory_args = [args.native_sources, args.homebrew_cellar, args.mpv_build, args.angle_sdk, args.angle_receipt]
    if any(inventory_args) and not all(inventory_args):
        parser.error("Native sources require --native-sources, --homebrew-cellar, --mpv-build, --angle-sdk and --angle-receipt")
    try:
        assemble(args)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Packaging failed: {error}\n")


if __name__ == "__main__":
    main()
