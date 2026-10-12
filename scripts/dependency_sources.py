"""Checksum-verified dependency archives and their original license notices.

Shared by the application inventory and Rust code embedded in native packages.
Archives and recipes are read as data; no dependency build scripts are executed.
"""
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import urllib.request

from native_evidence import digest
from source_notices import is_notice

# This release omitted repository and notice files. Its original manifest and
# every source file were compared with sys/ at this exact upstream commit.
# Version/revision matching makes dependency upgrades require a new review.
REPOSITORY_OVERRIDES = {
    ("libappindicator-sys", "0.9.0", "eafd1e3682a1247f595410266091e9684021cb6f"):
        "https://github.com/tauri-apps/libappindicator-rs",
}

# These 2018 releases predate Cargo's VCS receipt and omitted their licenses.
# The published payloads (including import archives) match this release commit,
# which also preserves the definition files and Cargo build scripts. Match every
# payload again when collecting; a version string alone is not provenance.
LEGACY_NOTICE_SOURCES = {
    ("winapi-i686-pc-windows-gnu", "0.4.0"): {
        "sha256": "ac3b87c63620426dd9b991e5ce0329eff545bccbbb34f3be09ff6fb6ab51b7b6",
        "commit": "9497609ef44cc9bcd16cd2411c0ee6ccaf5483aa", "directory": "i686"},
    ("winapi-x86_64-pc-windows-gnu", "0.4.0"): {
        "sha256": "712e227841d057c1ee1cd2fb22fa7e5a5461ae8e48fa2ca79ec42cfc1931183f",
        "commit": "9497609ef44cc9bcd16cd2411c0ee6ccaf5483aa", "directory": "x86_64"},
}


def verify_legacy_payload(crate, repository, directory):
    """Bind an old crate's sources and prebuilt inputs to a reviewed Git tree."""
    matched = 0
    with tarfile.open(crate) as published, tarfile.open(repository) as source:
        root = source.getnames()[0].split('/')[0]
        for member in published:
            if not member.isfile():
                continue
            relative = Path(member.name)
            if relative.is_absolute() or '..' in relative.parts or len(relative.parts) < 2:
                raise ValueError(f'Unsafe legacy crate path: {member.name}')
            name = Path(*relative.parts[1:]).as_posix()
            if name == 'Cargo.toml':
                continue  # Cargo's generated registry manifest; compare its original below.
            if name == 'Cargo.toml.orig':
                name = 'Cargo.toml'
            try:
                original = source.extractfile(f'{root}/{directory}/{name}')
            except KeyError as error:
                raise ValueError(f'Legacy crate payload missing from source: {name}') from error
            with published.extractfile(member) as payload, original:
                if hashlib.file_digest(payload, 'sha256').digest() != hashlib.file_digest(original, 'sha256').digest():
                    raise ValueError(f'Legacy crate payload differs from source: {name}')
            matched += 1
    if not matched:
        raise ValueError('Empty legacy crate payload')
    return matched


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
            if not (is_notice(member.name)
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
    legacy = None
    with tarfile.open(crate) as source:
        member = f"{package['name']}-{package['version']}/.cargo_vcs_info.json"
        try:
            vcs = json.load(source.extractfile(member))
        except KeyError as error:
            legacy = LEGACY_NOTICE_SOURCES.get((package['name'], package['version']))
            if not legacy or digest(crate) != legacy['sha256']:
                raise ValueError(f"No notices or pinned upstream revision: {package['name']}") from error
            vcs = {'git': {'sha1': legacy['commit']}}
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
                          "provenance": ("Reviewed legacy revision with exact published payload comparison" if legacy
                                         else "Git revision from the checksum-verified published crate"),
                          "repository_origin": "Reviewed version-specific override" if override else "Cargo metadata"}
    if legacy:
        matched = verify_legacy_payload(crate, output / 'sources/upstream' / captured[name]['file'],
                                        legacy['directory'])
        captured[name].setdefault('verified_legacy_payloads', {})[package['name']] = {
            'crate_sha256': legacy['sha256'], 'directory': legacy['directory'], 'matched_files': matched}
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
