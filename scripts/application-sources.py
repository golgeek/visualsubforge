#!/usr/bin/env python3
"""Capture target-specific Rust/frontend provenance and checksum-matched sources.

Native media/OS inventories are separate. Cargo's non-dev dependency closure
contains runtime code and build/proc-macro inputs; it does not assert that every
symbol survived linking. Frontend packages come from actual production chunks.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tomllib
import urllib.request

from native_evidence import digest
from source_archive import archive_source

ROOT = Path(__file__).resolve().parent.parent

# This release omitted repository and notice files. Its original manifest and
# every source file were compared with sys/ at this exact upstream commit.
# Version/revision matching makes dependency upgrades require a new review.
REPOSITORY_OVERRIDES = {
    ("libappindicator-sys", "0.9.0", "eafd1e3682a1247f595410266091e9684021cb6f"):
        "https://github.com/tauri-apps/libappindicator-rs",
}


def capture(*args):
    return subprocess.check_output(list(map(str, args)), cwd=ROOT, text=True).strip()


def non_dev_closure(metadata, names):
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    roots = {package["id"] for package in packages.values()
             if package["name"] in names and package["source"] is None}
    if {packages[key]["name"] for key in roots} != set(names):
        raise ValueError("Missing application package roots")
    seen, pending = set(), list(roots)
    while pending:
        package_id = pending.pop()
        if package_id in seen:
            continue
        seen.add(package_id)
        for edge in nodes[package_id]["deps"]:
            if any(kind["kind"] != "dev" for kind in edge["dep_kinds"]):
                pending.append(edge["pkg"])
    return [packages[key] for key in sorted(seen)], nodes


def verified_download(url, destination, algorithm, expected, cached=None):
    if destination.exists():
        raise ValueError(f"Refusing to overwrite {destination}")
    if cached and cached.is_file():
        shutil.copy2(cached, destination)
    else:
        with urllib.request.urlopen(url, timeout=120) as response, destination.open("xb") as target:
            shutil.copyfileobj(response, target)
    with destination.open("rb") as stream:
        actual = hashlib.file_digest(stream, algorithm).digest()
    if actual != expected:
        raise ValueError(f"Source checksum mismatch: {url}")
    return {"url": url, "file": destination.name, "sha256": digest(destination)}


def source_notices(archive, destination, license_file=None):
    """Copy notices only, without extracting executable sources or symlinks."""
    notices = []
    with tarfile.open(archive) as source:
        for member in source:
            relative = Path(member.name)
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError(f"Unsafe source archive path: {member.name}")
            if not member.isfile():
                continue
            if not (re.match(r"^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-]|$)", relative.name, re.I)
                    or (license_file and member.name.endswith("/" + license_file))):
                continue
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(member) as original, target.open("xb") as copied:
                shutil.copyfileobj(original, copied)
            notices.append({"path": relative.as_posix(), "sha256": digest(target)})
    return notices


def upstream_notices(package, crate, output, captured):
    # Cargo packages can omit their monorepo's root LICENSE. The locked .crate
    # identifies its exact Git revision; retain that additional source archive
    # and its notices instead of substituting today's license or a generic text.
    with tarfile.open(crate) as source:
        member = f"{package['name']}-{package['version']}/.cargo_vcs_info.json"
        try:
            vcs = json.load(source.extractfile(member))
        except KeyError as error:
            raise ValueError(f"No notices or pinned upstream revision: {package['name']}") from error
    commit = vcs["git"]["sha1"]
    override = REPOSITORY_OVERRIDES.get((package["name"], package["version"], commit))
    repository = (package.get("repository") or override or "").removesuffix(".git")
    match = re.fullmatch(r"https://github.com/([\w.-]+/[\w.-]+)", repository)
    if not match or not re.fullmatch(r"[0-9a-f]{40}", commit) or vcs["git"].get("dirty"):
        raise ValueError(f"Unreviewed upstream notice source: {package['name']}: {repository}")
    name = match[1].replace("/", "-") + "-" + commit
    if name not in captured:
        directory = output / "sources/upstream"
        directory.mkdir(exist_ok=True)
        archive = directory / (name + ".tar.gz")
        url = f"https://codeload.github.com/{match[1]}/tar.gz/{commit}"
        with urllib.request.urlopen(url, timeout=120) as response, archive.open("xb") as stream:
            shutil.copyfileobj(response, stream)
        notices = source_notices(archive, output / "notices/upstream" / name)
        if not notices:
            raise ValueError(f"Pinned upstream has no notices: {name}")
        captured[name] = {"file": archive.name, "url": url, "commit": commit,
                          "sha256": digest(archive), "notices": notices,
                          "provenance": "Git revision from the checksum-verified published crate",
                          "repository_origin": "Reviewed version-specific override" if override else "Cargo metadata"}
    return name


def verify_unpacked_crate(archive, unpacked):
    with tarfile.open(archive) as published:
        for member in published:
            if not member.isfile():
                continue
            parts = Path(member.name).parts
            if len(parts) < 2 or ".." in parts or Path(member.name).is_absolute():
                raise ValueError(f"Unsafe crate path: {member.name}")
            actual = unpacked.joinpath(*parts[1:])
            with published.extractfile(member) as stream:
                expected = hashlib.file_digest(stream, "sha256").hexdigest()
            if digest(actual) != expected:
                raise ValueError(f"Compiler input differs from published crate: {member.name}")


def collect_rust(output, target, features):
    command = ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target,
               "--features", features]
    metadata = json.loads(capture(*command))
    packages, nodes = non_dev_closure(metadata, ["visualsubforge", "vsf-cli"])
    locked = tomllib.loads((ROOT / "Cargo.lock").read_text())
    checksums = {(entry["name"], entry["version"], entry.get("source")): entry.get("checksum")
                 for entry in locked["package"]}
    sources = output / "sources/rust"
    sources.mkdir(parents=True)
    records, upstream = [], {}
    for package in packages:
        name, version = package["name"], package["version"]
        record = {"name": name, "version": version, "license": package["license"],
                  "features": nodes[package["id"]]["features"]}
        if package["source"] is None:
            relative = Path(package["manifest_path"]).resolve().relative_to(ROOT)
            record["workspace_manifest"] = relative.as_posix()
        else:
            if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
                raise ValueError(f"Unimplemented source provenance: {package['source']}")
            checksum = checksums[(name, version, package["source"])]
            if not checksum:
                raise ValueError(f"Missing Cargo.lock checksum: {name} {version}")
            filename = f"{name}-{version}.crate"
            unpacked = Path(package["manifest_path"]).parent
            cached = unpacked.parent.parent.parent / "cache" / unpacked.parent.name / filename
            archive = sources / filename
            record["source"] = verified_download(f"https://static.crates.io/crates/{name}/{filename}",
                                                archive, "sha256", bytes.fromhex(checksum), cached)
            verify_unpacked_crate(archive, unpacked)
            record["notices"] = source_notices(archive, output / "notices/rust", package.get("license_file"))
            if not record["notices"]:
                record["upstream_notices"] = upstream_notices(package, archive, output, upstream)
        records.append(record)
    return {"command": command, "target": target, "features": features,
            "scope": "Non-dev resolved GUI/CLI dependency closure, including compile-time inputs",
            "packages": records, "upstream_sources": upstream}


def collect_frontend(output, frontend):
    inventory = json.loads((frontend / "frontend-inventory.json").read_text())
    lock = json.loads((ROOT / "package-lock.json").read_text())
    sources = output / "sources/npm"
    sources.mkdir(parents=True)
    for package in inventory["packages"]:
        entry = lock["packages"][package["path"]]
        if entry["version"] != package["version"]:
            raise ValueError(f"Frontend version differs from lockfile: {package['name']}")
        algorithm, checksum = entry["integrity"].split("-", 1)
        if algorithm not in ("sha256", "sha512"):
            raise ValueError(f"Unsupported npm integrity algorithm: {algorithm}")
        filename = package["name"].replace("/", "-").replace("@", "") + "-" + package["version"] + ".tgz"
        archive = sources / filename
        package["source"] = verified_download(entry["resolved"], archive, algorithm, base64.b64decode(checksum))
        # Compare inputs observed by Vite with the matching published source.
        # Equal package.json versions alone would miss locally modified modules.
        prefix = package["path"] + "/"
        expected = {module["id"].removeprefix(prefix): module["sha256"]
                    for chunk in inventory["chunks"] for module in chunk["modules"]
                    if module["id"].startswith(prefix) and "sha256" in module}
        expected.update({notice["path"]: notice["sha256"] for notice in package["notices"]})
        with tarfile.open(archive) as published:
            for name, checksum in expected.items():
                with published.extractfile("package/" + name) as stream:
                    if hashlib.file_digest(stream, "sha256").hexdigest() != checksum:
                        raise ValueError(f"Bundled input differs from published source: {package['name']}/{name}")
        notices = source_notices(archive, output / "notices/npm" / filename.removesuffix(".tgz"))
        if not notices:
            raise ValueError(f"No published source notices: {package['name']}")
    # Hash files after Vite has finished all hooks, including this inventory.
    inventory["assets_sha256"] = {file.relative_to(frontend).as_posix(): digest(file)
                                  for file in sorted(frontend.rglob("*")) if file.is_file()}
    return inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", required=True)
    parser.add_argument("--features", required=True, help="The GUI and CLI's qualified Cargo feature names")
    parser.add_argument("--frontend", type=Path, default=ROOT / "dist")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    receipt = {"schema": 1, "status": "incomplete", "commit": capture("git", "rev-parse", "HEAD"),
               "tracked_changes": capture("git", "diff", "--name-only", "HEAD").splitlines(),
               "lockfiles": {name: digest(ROOT / name) for name in ("Cargo.lock", "package-lock.json")},
               "rustc": capture("rustc", "-vV"), "node": capture("node", "--version"),
               "scope": "Application source inputs; native media and platform runtime inventories are separate"}
    try:
        receipt["rust"] = collect_rust(output, args.target, args.features)
        receipt["frontend"] = collect_frontend(output, args.frontend.resolve(strict=True))
        # Only tracked application files; intentionally untracked context stays private.
        archive_source(ROOT, output / "visualsubforge-source.tar.gz")
        receipt["project_source_sha256"] = digest(output / "visualsubforge-source.tar.gz")
        receipt["status"] = "captured"
    finally:
        (output / "application-inventory.json").write_text(json.dumps(receipt, indent=2) + "\n")
    files = [file for file in sorted(output.rglob("*")) if file.is_file()]
    (output / "SHA256SUMS").write_text("".join(f"{digest(file)}  {file.relative_to(output).as_posix()}\n" for file in files))
    print(f"Captured {len(receipt['rust']['packages'])} Rust packages and {len(receipt['frontend']['packages'])} frontend inputs")


if __name__ == "__main__":
    main()
