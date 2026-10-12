#!/usr/bin/env python3
"""Assemble an Ubuntu 24.04 amd64 .deb with private media tools and libraries.

GTK/WebKit, libc and display/audio drivers stay with the OS. Their package
dependencies are resolved by apt; media libraries outside that runtime closure
are copied privately. This is a validation artifact until release gates pass.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
from native_evidence import digest, verified_headers
from ubuntu_native_sources import collect as collect_ubuntu_inputs


ROOT = Path(__file__).resolve().parent.parent
PLATFORM_LIBRARIES = (
    "libwebkit2gtk-4.1.so.0", "libgtk-3.so.0", "libepoxy.so.0",
    "libGL.so.1", "libEGL.so.1", "libpulse.so.0", "libasound.so.2",
    "libc.so.6", "libstdc++.so.6", "libgcc_s.so.1",
)


def capture(*command, env=None):
    return subprocess.check_output(list(map(str, command)), text=True, env=env).strip()


def parse_ldd(text):
    if "not found" in text:
        raise ValueError(f"Unresolved ELF dependency:\n{text}")
    # The dynamic loader appears as an absolute path without the usual SONAME
    # arrow, and some libraries also name it in DT_NEEDED. Keep it in the OS
    # closure instead of silently dropping it from ldd's observed graph.
    result = {}
    for match in re.finditer(r"^\s*(?:(\S+) => )?(/[^\n]+?) \(0x[0-9a-f]+\)", text, re.M):
        path = Path(match[2])
        result[match[1] or path.name] = path
    return result


def dependencies(path, env):
    return {name: library.resolve(strict=True)
            for name, library in parse_ldd(capture("ldd", path, env=env)).items()}


def package_owner(path):
    # Ubuntu's merged /usr may be recorded in dpkg using either spelling.
    paths = [str(path)]
    if str(path).startswith("/usr/"):
        paths.append(str(path).removeprefix("/usr"))
    for candidate in paths:
        result = subprocess.run(["dpkg-query", "-S", candidate], text=True, capture_output=True)
        if result.returncode == 0:
            owners = {line.rsplit(": ", 1)[0] for line in result.stdout.splitlines()}
            if len(owners) != 1:
                raise ValueError(f"Ambiguous package owner: {path}: {owners}")
            package = owners.pop()
            fields = capture("dpkg-query", "-W", "-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}", package).split("\t")
            return dict(zip(("package", "version", "source", "source_version"), fields, strict=True))
    raise ValueError(f"No installed package owns {path}")


def platform_runtime(env):
    cache = capture("/usr/sbin/ldconfig", "-p")
    paths, packages = set(), {}
    for name in PLATFORM_LIBRARIES:
        matches = re.findall(r"^\s*" + re.escape(name) + r" \(libc6,x86-64[^)]*\) => (.+)$", cache, re.M)
        resolved = {Path(path).resolve(strict=True) for path in matches}
        if len(resolved) != 1:
            raise ValueError(f"Expected one amd64 platform library: {name}: {resolved}")
        library = resolved.pop()
        paths.add(library)
        paths.update(dependencies(library, env).values())
        owner = package_owner(library)
        packages[owner["package"]] = owner["version"]
    return paths, packages


def collect_distro_sources(packages, destination, notices):
    """apt verifies downloads against the signed source index, including patches."""
    destination.mkdir()
    for package in packages.values():
        name = package["package"].split(":")[0]
        copyright_file = Path("/usr/share/doc") / name / "copyright"
        shutil.copy2(copyright_file.resolve(strict=True), notices / (name + ".copyright"))
    sources = {(p["source"], p["source_version"]) for p in packages.values()}
    for name, version in sorted(sources):
        subprocess.run(["apt-get", "source", "--download-only", "--only-source", f"{name}={version}"],
                       cwd=destination, check=True)
    shutil.copytree("/usr/share/common-licenses", notices / "common-licenses")


def assemble(args):
    if capture("dpkg", "--print-architecture") != "amd64":
        raise ValueError("The initial Linux preview targets Ubuntu 24.04 amd64")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    tree = root / "root"
    private = tree / "usr/lib/visualsubforge"
    private.mkdir(parents=True)
    libraries = private / "lib"
    libraries.mkdir()
    doc = tree / "usr/share/doc/visualsubforge"
    notices = doc / "third-party"
    notices.mkdir(parents=True)
    application_inventory = None
    if getattr(args, "application_sources", None):
        application = args.application_sources.resolve(strict=True)
        application_inventory = digest(application / "application-inventory.json")
        shutil.copytree(application / "notices", notices / "application")
        shutil.copy2(application / "application-inventory.json", doc / "application-inventory.json")
    env = {key: value for key, value in os.environ.items() if not key.startswith(("LD_", "DYLD_"))}
    env["LC_ALL"] = "C"
    platform, platform_packages = platform_runtime(env)
    build_env = {**env, "LD_LIBRARY_PATH": ":".join(str(p.resolve(strict=True)) for p in args.library_dir)}
    roots = {getattr(args, name).resolve(strict=True): Path(name)
             for name in ("visualsubforge", "vsf", "ffmpeg", "ffprobe")}
    destinations = dict(roots)
    queue = list(roots)
    while queue:
        original = queue.pop()
        if "Advanced Micro Devices X86-64" not in capture("readelf", "-h", original, env=env):
            raise ValueError(f"Not an amd64 ELF binary: {original}")
        resolved = dependencies(original, build_env)
        for name in capture("patchelf", "--print-needed", original).splitlines():
            dependency = resolved[name]
            if dependency in platform:
                continue
            relative = Path("lib") / name
            if dependency in destinations:
                if destinations[dependency] != relative:
                    raise ValueError(f"Conflicting names for {dependency}")
                continue
            if relative in destinations.values():
                raise ValueError(f"Conflicting library basename: {name}")
            destinations[dependency] = relative
            queue.append(dependency)
    records, distro = [], {}
    custom_dirs = [path.resolve(strict=True) for path in args.library_dir]
    for original, relative in destinations.items():
        copied = private / relative
        shutil.copy2(original, copied)
        copied.chmod(0o755)
        subprocess.run(["patchelf", "--set-rpath", "$ORIGIN/lib" if relative.parent == Path(".") else "$ORIGIN", str(copied)], check=True)
        record = {"path": str(relative), "source": str(original), "source_sha256": digest(original), "sha256": digest(copied)}
        application = relative in (Path("visualsubforge"), Path("vsf"))
        if not application and not any(original.is_relative_to(path) for path in custom_dirs):
            owner = package_owner(original)
            record["distro"] = owner
            distro[owner["package"]] = owner
        records.append(record)
    # Audit the copied graph with no build-time library overrides. RUNPATH is
    # attached to each library, since executable RUNPATH is not transitive.
    for relative in destinations.values():
        for name, path in dependencies(private / relative, env).items():
            if path not in platform and not path.is_relative_to(private):
                raise ValueError(f"Package still resolves outside its runtime: {relative}: {name} -> {path}")
    header_inputs = []
    if getattr(args, "placebo_build", None):
        build_receipt = args.placebo_build / "build-receipt.json"
        for header in verified_headers(build_receipt):
            owner = package_owner(Path(header["path"]))
            distro[owner["package"]] = owner
            header_inputs.append({**header, "distro": owner, "used_by": "libplacebo embedded header"})
        shutil.copy2(build_receipt, root / "libplacebo-build.json")
    collect_distro_sources(distro, root / "distro-sources", notices)
    embedded_sources = root / "distro-sources/build-inputs"
    collect_ubuntu_inputs(records, embedded_sources, notices / "build-inputs")
    shutil.copy2(embedded_sources / "inventory.json", doc / "ubuntu-build-inputs.json")
    source_records = []
    for item in args.native_source:
        name, path = item.split("=", 1)
        if not re.fullmatch(r"[a-z0-9-]+", name):
            raise ValueError("Native source names must be simple lowercase names")
        source = Path(path).resolve(strict=True)
        target = root / f"{name}-source.tar.gz"
        shutil.copy2(source, target)
        source_records.append({"component": name, "file": target.name, "sha256": digest(target)})
    for item in args.native_notices:
        name, path = item.split("=", 1)
        target = notices / name
        target.mkdir()
        source = Path(path).resolve(strict=True)
        for path in source.rglob("*"):
            if path.is_file() and ".git" not in path.parts and re.match(r"^(COPYING|LICENSE|NOTICE|Copyright)", path.name, re.I):
                copied = target / path.relative_to(source)
                copied.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, copied)
    shutil.copy2(ROOT / "LICENSE", doc / "copyright")
    for name in ("visualsubforge", "vsf"):
        wrapper = tree / "usr/bin" / name
        wrapper.parent.mkdir(parents=True, exist_ok=True)
        wrapper.write_text(f'#!/bin/sh\nexec /usr/lib/visualsubforge/{name} "$@"\n')
        wrapper.chmod(0o755)
    desktop = tree / "usr/share/applications/visualsubforge.desktop"
    desktop.parent.mkdir(parents=True)
    desktop.write_text("[Desktop Entry]\nType=Application\nName=VisualSubForge\nExec=visualsubforge\nIcon=visualsubforge\nCategories=AudioVideo;Video;\nTerminal=false\n")
    icon = tree / "usr/share/icons/hicolor/512x512/apps/visualsubforge.png"
    icon.parent.mkdir(parents=True)
    shutil.copy2(ROOT / "src-tauri/icons/icon.png", icon)
    depends = [f"{name} (>= {version})" for name, version in sorted(platform_packages.items())]
    depends += ["xdg-desktop-portal", "xdg-desktop-portal-gtk", "fonts-dejavu-core"]
    control = tree / "DEBIAN/control"
    control.parent.mkdir()
    control.write_text("Package: visualsubforge\nVersion: 0.0.1~alpha\nArchitecture: amd64\n"
                       "Maintainer: VisualSubForge contributors <noreply@github.com>\n"
                       "Section: video\nPriority: optional\n" + "Depends: " + ", ".join(depends) + "\n"
                       "Description: VisualSubForge subtitle editor and resync CLI (preview)\n")
    receipt = {"schema": 1, "version": "0.0.1-alpha", "architecture": "amd64", "os": "Ubuntu 24.04",
               "commit": capture("git", "-C", ROOT, "rev-parse", "HEAD"),
               "files": records, "header_inputs": header_inputs,
               "platform_dependencies": depends, "native_sources": source_records,
               "application_inventory_sha256": application_inventory,
               "ubuntu_build_inputs_sha256": digest(embedded_sources / "inventory.json"),
               "redistribution": "Pending complete Rust/frontend/native license review",
               "validation": "Assembly and ELF closure only; not installation/GPU/audio acceptance"}
    (doc / "native-runtime.json").write_text(json.dumps(receipt, indent=2) + "\n")
    artifact = root / "VisualSubForge_0.0.1-alpha_amd64_ubuntu24.04.deb"
    subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(tree), str(artifact)], check=True)
    receipt["artifact"] = {"file": artifact.name, "sha256": digest(artifact)}
    (root / "assembly-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    source_files = [path for path in (root / "distro-sources").rglob("*") if path.is_file()]
    source_files += list(root.glob("*-source.tar.gz"))
    (root / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.relative_to(root)}\n" for path in [artifact, *sorted(source_files)]))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("visualsubforge", "vsf", "ffmpeg", "ffprobe", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--library-dir", type=Path, action="append", default=[])
    parser.add_argument("--native-source", action="append", default=[], help="name=source.tar.gz")
    parser.add_argument("--native-notices", action="append", default=[], help="name=source-directory")
    parser.add_argument("--application-sources", type=Path)
    parser.add_argument("--placebo-build", type=Path)
    try:
        assemble(parser.parse_args())
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Linux packaging failed: {error}\n")
