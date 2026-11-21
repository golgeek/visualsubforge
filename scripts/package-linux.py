#!/usr/bin/env python3
"""Assemble an Ubuntu 24.04 amd64 .deb with private media tools and libraries.

GTK/WebKit, libc and display/audio drivers stay with the OS. Their package
dependencies are resolved by apt. Every shipped ELF directly links only the
listed system SONAMEs and the private media libraries.
"""
from release_version import release_version, source_inventory_reference

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
# Direct DT_NEEDED boundary of shipped ELF files. System libraries manage
# their own transitive dependencies (including the distribution's JPEG/XML).
PLATFORM_LIBRARIES = frozenset("""
ld-linux-x86-64.so.2 libc.so.6 libm.so.6 libdl.so.2 libpthread.so.0 librt.so.1
libstdc++.so.6 libgcc_s.so.1 libatomic.so.1 libwebkit2gtk-4.1.so.0
libjavascriptcoregtk-4.1.so.0 libsoup-3.0.so.0 libgtk-3.so.0 libgdk-3.so.0
libglib-2.0.so.0 libgobject-2.0.so.0 libgio-2.0.so.0 libgdk_pixbuf-2.0.so.0
libcairo.so.2 libdbus-1.so.3 libpango-1.0.so.0 libpangocairo-1.0.so.0
libepoxy.so.0 libGL.so.1 libEGL.so.1 libvulkan.so.1 libgbm.so.1 libdrm.so.2
libva.so.2 libva-drm.so.2 libva-x11.so.2 libva-wayland.so.2 libX11.so.6 libXext.so.6 libxcb.so.1 libwayland-client.so.0
libpulse.so.0 libasound.so.2 libfreetype.so.6 libfribidi.so.0 libharfbuzz.so.0
libfontconfig.so.1 libz.so.1 liblzma.so.5 liblcms2.so.2
""".split())
DISTRO_BUNDLED = {"libass.so.9", "libunibreak.so.5", "libshaderc.so.1",
                  "libspirv-cross-c-shared.so.0"}


def family(name):
    return name.split(".so", 1)[0]



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
    # Detect collisions with libraries that WebKit or drivers already require.
    for name in ("libwebkit2gtk-4.1.so.0", "libepoxy.so.0", "libpulse.so.0", "libasound.so.2"):
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
        raise ValueError("The Linux package targets Ubuntu 24.04 amd64")
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
    custom_dirs = [path.resolve(strict=True) for path in args.library_dir]
    if getattr(args, "spirv_cross", None):
        library = args.spirv_cross.resolve(strict=True)
        name = capture("patchelf", "--print-soname", library)
        if name != "libspirv-cross-c-shared.so.0":
            raise ValueError(f"Unexpected SPIRV-Cross ABI: {name}")
        destinations[library] = Path("lib") / name
    system_families = {family(path.name) for path in platform} | {family(name) for name in PLATFORM_LIBRARIES}
    system_links = {}
    linked_platform_files = set()
    queue = list(destinations)
    while queue:
        original = queue.pop()
        if "Advanced Micro Devices X86-64" not in capture("readelf", "-h", original, env=env):
            raise ValueError(f"Not an amd64 ELF binary: {original}")
        resolved = dependencies(original, build_env)
        for name in capture("patchelf", "--print-needed", original).splitlines():
            dependency = resolved[name]
            if name in PLATFORM_LIBRARIES:
                if any(dependency.is_relative_to(path) for path in custom_dirs):
                    raise ValueError(f"System SONAME resolves to SDK: {name} -> {dependency}")
                linked_platform_files.add(dependency)
                system_links[name] = str(dependency)
                continue
            if family(name) in system_families:
                raise ValueError(f"Bundled family overlaps system runtime: {name}")
            if not any(dependency.is_relative_to(path) for path in custom_dirs) and name not in DISTRO_BUNDLED:
                raise ValueError(f"Unreviewed system library: {original}: {name} -> {dependency}")
            relative = Path("lib") / name
            if dependency in destinations:
                if destinations[dependency] != relative:
                    raise ValueError(f"Conflicting names for {dependency}")
                continue
            if relative in destinations.values():
                raise ValueError(f"Conflicting library basename: {name}")
            destinations[dependency] = relative
            queue.append(dependency)
    if getattr(args, "build_paths", None):
        required_va = {"libva.so.2", "libva-drm.so.2", "libva-x11.so.2"}
        if missing := required_va - system_links.keys():
            raise ValueError(f"Media SDK is missing required VA-API loader links: {sorted(missing)}")
    bundled_families = {}
    for original, relative in destinations.items():
        if original in roots:
            continue
        soname = capture("patchelf", "--print-soname", original)
        if soname != relative.name or family(soname) in system_families:
            raise ValueError(f"Unexpected private SONAME: {original}: {soname}")
        previous = bundled_families.setdefault(family(soname), soname)
        if previous != soname:
            raise ValueError(f"Multiple private ABIs: {previous} and {soname}")
    for library in sorted(linked_platform_files):
        owner = package_owner(library)
        platform_packages[owner["package"]] = owner["version"]
    records, distro = [], {}
    for original, relative in destinations.items():
        copied = private / relative
        shutil.copy2(original, copied)
        copied.chmod(0o755)
        subprocess.run(["patchelf", "--set-rpath", "$ORIGIN/lib" if relative.parent == Path(".") else "$ORIGIN", str(copied)], check=True)
        record = {"path": str(relative), "source": str(original), "source_sha256": digest(original), "sha256": digest(copied),
                  "needed": capture("patchelf", "--print-needed", copied).splitlines()}
        if original not in roots and not any(original.is_relative_to(path) for path in custom_dirs):
            owner = package_owner(original)
            record["distro"] = owner
            distro[owner["package"]] = owner
        records.append(record)
    # Audit the copied graph with no build-time library overrides. RUNPATH is
    # attached to each library, since executable RUNPATH is not transitive.
    for relative in destinations.values():
        copied = private / relative
        resolved = dependencies(copied, env)
        for name in capture("patchelf", "--print-needed", copied).splitlines():
            path = resolved[name]
            if name in PLATFORM_LIBRARIES:
                if str(path) != system_links[name]:
                    raise ValueError(f"Changed system provider: {name} -> {path}")
            elif path != libraries / name:
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
        extension = ".tar.xz" if source.name.endswith(".tar.xz") else ".tar.gz"
        target = root / f"{name}-source{extension}"
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
    debian_version = release_version().replace("-", "~", 1)
    control.write_text(f"Package: visualsubforge\nVersion: {debian_version}\nArchitecture: amd64\n"
                       "Maintainer: VisualSubForge contributors <noreply@github.com>\n"
                       "Section: video\nPriority: optional\n" + "Depends: " + ", ".join(depends) + "\n"
                       "Description: VisualSubForge subtitle editor and resync CLI (test release)\n This test release supports Ubuntu 24.04 amd64.\n")
    receipt = {"schema": 1, "version": release_version(), "architecture": "amd64", "os": "Ubuntu 24.04",
               "commit": capture("git", "-C", ROOT, "rev-parse", "HEAD"),
               "files": records, "header_inputs": header_inputs, "system_libraries": system_links,
               "platform_dependencies": depends, "native_sources": source_records,
               "application_inventory_sha256": application_inventory,
               "ubuntu_build_inputs_sha256": digest(embedded_sources / "inventory.json"),
               "redistribution": source_inventory_reference(release_version(), "linux-amd64"),
               "validation": "Assembly verified ELF dependencies and recorded file checksums."}
    (doc / "native-runtime.json").write_text(json.dumps(receipt, indent=2) + "\n")
    artifact = root / f"VisualSubForge_{release_version()}_amd64_ubuntu24.04.deb"
    subprocess.run(["dpkg-deb", "--root-owner-group", "--build", str(tree), str(artifact)], check=True)
    receipt["artifact"] = {"file": artifact.name, "sha256": digest(artifact)}
    (root / "assembly-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    source_files = [path for path in (root / "distro-sources").rglob("*") if path.is_file()]
    source_files += list(root.glob("*-source.tar.*"))
    (root / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.relative_to(root)}\n" for path in [artifact, *sorted(source_files)]))


def use_build_paths(args):
    paths = json.loads(args.build_paths.read_text())
    mpv = Path(paths["VSF_MPV_BUILD"])
    placebo = Path(paths["VSF_PLACEBO_BUILD"])
    ffmpeg = Path(paths["VSF_FFMPEG_BUILD"])
    dovi = Path(paths["VSF_DOVI_BUILD"])
    args.ffmpeg = ffmpeg / "sdk/bin/ffmpeg"
    args.ffprobe = ffmpeg / "sdk/bin/ffprobe"
    args.library_dir = [mpv / "build", *(p / "sdk/lib" for p in (placebo, ffmpeg, dovi))]
    args.spirv_cross = Path(capture("pkg-config", "--variable=libdir", "spirv-cross-c-shared")) / "libspirv-cross-c-shared.so"
    args.placebo_build = placebo
    components = [
        ("mpv", mpv / "source.tar.gz", mpv / "prepared/source"),
        ("libplacebo", placebo / "source.tar.gz", placebo / "source"),
        ("libdovi", dovi / "source.tar.gz", dovi / "source/dolby_vision"),
        ("bzip2", ffmpeg / "bzip2-source.tar.gz", ffmpeg / "bzip2"),
        ("ffmpeg", ffmpeg / "ffmpeg.tar.xz", ffmpeg / "source"),
        ("dav1d", ffmpeg / "dav1d.tar.xz", ffmpeg / "dav1d"),
    ]
    args.native_source = [f"{name}={archive}" for name, archive, _ in components]
    args.native_notices = [f"{name}={directory}" for name, _, directory in components]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("visualsubforge", "vsf", "build-paths", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--application-sources", type=Path)
    try:
        args = parser.parse_args()
        use_build_paths(args)
        assemble(args)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Linux packaging failed: {error}\n")
