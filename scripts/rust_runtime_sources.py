"""Verify installed Rust runtime inputs against their exact distribution sources."""
import hashlib
from pathlib import Path
import shutil
import tarfile
import tomllib

from dependency_sources import source_notices, verified_download
from native_evidence import digest


def collect(output, target, sysroot):
    """Capture rustup's exact standard-library component and vendored sources.

    Cargo.lock does not describe libstd. Verify the installed link inputs against
    their distribution archive instead of inferring them from rustc's version.
    """
    manifest_path = sysroot / "lib/rustlib/multirust-channel-manifest.toml"
    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    sources = output / "sources/rust-toolchain"
    sources.mkdir(parents=True)
    shutil.copy2(manifest_path, sources / "distribution-manifest.toml")
    receipt = {"target": target, "distribution_date": manifest["date"],
               "manifest_sha256": digest(manifest_path), "components": {}}
    std_files = {}
    for name, component_target in [("rust-src", "*"), ("rust-std", target)]:
        package = manifest["pkg"][name]
        component = package["target"][component_target]
        if not component["available"]:
            raise ValueError(f"Rust distribution component unavailable: {name} {component_target}")
        url = component["xz_url"]
        archive = sources / url.rsplit("/", 1)[1]
        record = verified_download(url, archive, "sha256", bytes.fromhex(component["xz_hash"]))
        record["version"] = package["version"]
        record["notices"] = source_notices(archive, output / "notices/rust-toolchain" / name)
        if not record["notices"]:
            raise ValueError(f"Missing Rust component notices: {name}")
        receipt["components"][name] = record
        if name == "rust-src":
            # Include identities for vendored dependencies, including build/test
            # inputs. These are not all necessarily linked into this target.
            packages = []
            with tarfile.open(archive) as source:
                for member in source:
                    if member.isfile() and member.name.endswith("/Cargo.toml"):
                        data = tomllib.loads(source.extractfile(member).read().decode())
                        if package := data.get("package"):
                            packages.append({"manifest": member.name, "name": package.get("name"),
                                             "version": package.get("version"), "license": package.get("license")})
            record["source_packages"] = packages
        else:
            with tarfile.open(archive) as source:
                for member in source:
                    parts = Path(member.name).parts
                    if not member.isfile() or len(parts) < 4 or parts[2:4] != ("lib", "rustlib"):
                        continue
                    relative = Path(*parts[2:])
                    if ".." in relative.parts:
                        raise ValueError(f"Unsafe Rust component path: {member.name}")
                    with source.extractfile(member) as stream:
                        expected = hashlib.file_digest(stream, "sha256").hexdigest()
                    if digest(sysroot / relative) != expected:
                        raise ValueError(f"Installed Rust link input differs from distribution: {relative}")
                    std_files[relative.as_posix()] = expected
            if not std_files:
                raise ValueError("No Rust standard-library inputs were verified")
    receipt["installed_files_sha256"] = std_files
    receipt["scope"] = "Complete rust-std component and rust-src with vendored inputs; not a per-symbol link map"
    return receipt
