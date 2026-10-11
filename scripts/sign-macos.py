#!/usr/bin/env python3
"""Sign and notarize a copied VSF app using six Apple environment secrets.

Intended for the separate hosted signing job. No secret or subprocess argument
list enters the receipt. The source artifact is unchanged; failures retain a
receipt and clean up the private temporary Keychain and certificate export.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import plistlib
import secrets
import shlex
import subprocess
import tempfile


SECRET_NAMES = (
    "APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_SIGNING_IDENTITY",
    "APPLE_ID", "APPLE_TEAM_ID", "APPLE_APP_SPECIFIC_PASSWORD",
)
MACHO_MAGIC = {bytes.fromhex(value) for value in (
    "feedface", "cefaedfe", "feedfacf", "cffaedfe", "cafebabe", "bebafeca",
    "cafebabf", "bfbafeca",
)}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validated_files(app, manifest):
    """Reject a changed, incomplete or escaping artifact before using the key."""
    files = {}
    for record in manifest["files"]:
        relative = Path(record["file"])
        if relative.is_absolute() or ".." in relative.parts or relative in files:
            raise ValueError("Invalid or duplicate native inventory path")
        if relative.parent not in {Path("Contents/MacOS"), Path("Contents/Frameworks")}:
            raise ValueError("Native inventory path is outside the expected bundle layout")
        path = (app / relative).resolve(strict=True)
        if not path.is_relative_to(app) or digest(path) != record["bundled_sha256"]:
            raise ValueError(f"Native artifact differs from its assembly receipt: {relative}")
        files[relative] = path
    required = {Path("Contents/MacOS") / name for name in ("visualsubforge", "vsf", "ffmpeg", "ffprobe")}
    if not required <= files.keys():
        raise ValueError("Incomplete application/tool inventory")
    for path in app.rglob("*"):
        if path.is_symlink():
            if not path.resolve(strict=True).is_relative_to(app):
                raise ValueError("Bundle symlink escapes the application")
        elif path.is_file():
            with path.open("rb") as stream:
                native = stream.read(4) in MACHO_MAGIC
            if native and path.relative_to(app) not in files:
                raise ValueError(f"Uninventoried native code: {path.relative_to(app)}")
    return files


def validate_bundle_hashes(app, receipt):
    actual = {}
    for path in sorted(app.rglob("*")):
        if path.is_symlink():
            raise ValueError("Unexpected symlink in the assembled application")
        if path.is_file():
            actual[str(path.relative_to(app))] = digest(path)
    if actual != receipt["bundle_files_sha256"]:
        raise ValueError("Application files differ from the final assembly receipt")


def sign(args):
    source = args.app.resolve(strict=True)
    output = args.output.resolve()
    if output.exists() or output.is_relative_to(source):
        raise ValueError("Choose a new output directory outside the source app")
    credentials = {name: os.environ.get(name, "") for name in SECRET_NAMES}
    missing = [name for name, value in credentials.items() if not value]
    if missing:
        raise ValueError("Missing GitHub secrets: " + ", ".join(missing))
    identity = credentials["APPLE_SIGNING_IDENTITY"]
    team = credentials["APPLE_TEAM_ID"]
    if not identity.startswith("Developer ID Application: ") or not identity.endswith(f"({team})"):
        raise ValueError("Signing identity must be Developer ID Application for APPLE_TEAM_ID")
    assembly = json.loads(args.receipt.read_text())
    validate_bundle_hashes(source, assembly)
    validated_files(source, assembly)
    with (source / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info["CFBundleIdentifier"] != "com.golgeek.visualsubforge":
        raise ValueError("Unexpected application identifier")
    manifest_path = Path("Contents/Resources/native-runtime.json")
    manifest = json.loads((source / manifest_path).read_text())
    version = manifest["version"]
    if version != info["VSFReleaseVersion"] or any(c not in "0123456789abcdefghijklmnopqrstuvwxyz.-" for c in version):
        raise ValueError("Invalid or inconsistent application version")
    arch = {"arm64": "aarch64", "x86_64": "x86_64"}[manifest["architecture"]]
    encoded = "".join(credentials["APPLE_CERTIFICATE"].split())
    certificate = base64.b64decode(encoded, validate=True)
    output.mkdir(parents=True)
    app = output / "VisualSubForge.app"
    # ditto preserves macOS bundle metadata, including resource forks.
    subprocess.run(["ditto", str(source), str(app)], check=True)
    receipt = {"schema": 1, "status": "incomplete", "version": version,
               "architecture": arch, "minimum_macos": manifest["minimum_macos"],
               "redistribution_inventory": manifest["redistribution_inventory"],
               "scope": "Developer ID signature and Apple notarization; no clean-OS/GPU/audio acceptance"}
    key_password = secrets.token_urlsafe(32)
    redactions = [*credentials.values(), key_password]
    child_env = {key: value for key, value in os.environ.items() if key not in SECRET_NAMES}

    def run(command, timeout=180, status_json=False):
        # Do not use check=True: CalledProcessError renders the full argv,
        # including credentials for security/notarytool, in a traceback.
        result = subprocess.run(list(map(str, command)), env=child_env, stdin=subprocess.DEVNULL,
                                capture_output=True, text=True, timeout=timeout)
        if result.returncode:
            # notarytool can return a nonzero status with a structured result.
            # Preserve that status for rejection/timeout evidence and its ID.
            if status_json:
                try:
                    if "status" in json.loads(result.stdout):
                        return result.stdout
                except json.JSONDecodeError:
                    pass
            detail = result.stderr or result.stdout
            for value in redactions:
                detail = detail.replace(value, "[redacted]")
            raise RuntimeError(f"{Path(command[0]).name} failed ({result.returncode}): {detail[-4000:]}")
        return result.stdout

    try:
        with tempfile.TemporaryDirectory(prefix="vsf-signing-", dir=os.environ.get("RUNNER_TEMP")) as private:
            private = Path(private)
            keychain = private / "signing.keychain-db"
            p12 = private / "identity.p12"
            p12.write_bytes(certificate)
            p12.chmod(0o600)
            old_keychains = shlex.split(run(["security", "list-keychains", "-d", "user"]))
            created = False
            try:
                run(["security", "create-keychain", "-p", key_password, keychain])
                created = True
                run(["security", "set-keychain-settings", "-lut", "7200", keychain])
                run(["security", "unlock-keychain", "-p", key_password, keychain])
                run(["security", "import", p12, "-P", credentials["APPLE_CERTIFICATE_PASSWORD"],
                     "-k", keychain, "-t", "cert", "-f", "pkcs12", "-T", "/usr/bin/codesign"])
                run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
                     "-s", "-k", key_password, keychain])
                run(["security", "list-keychains", "-d", "user", "-s", keychain, *old_keychains])
                if f'"{identity}"' not in run(["security", "find-identity", "-v", "-p", "codesigning", keychain]):
                    raise ValueError("Imported P12 does not contain the configured signing identity/private key")
                profile = "vsf-ci-notary"
                run(["xcrun", "notarytool", "store-credentials", profile, "--keychain", keychain,
                     "--apple-id", credentials["APPLE_ID"], "--team-id", team,
                     "--password", credentials["APPLE_APP_SPECIFIC_PASSWORD"]])
                print("Signing identity and notarization credentials validated.", flush=True)
                validate_bundle_hashes(app, assembly)
                files = validated_files(app, assembly)
                main = Path("Contents/MacOS") / info["CFBundleExecutable"]
                for relative, path in files.items():
                    if relative != main:
                        run(["codesign", "--force", "--options", "runtime", "--timestamp",
                             "--sign", identity, "--keychain", keychain, path])
                manifest["signature"] = "Developer ID; notarization result is in the external signing receipt"
                (app / manifest_path).write_text(json.dumps(manifest, indent=2) + "\n")
                run(["codesign", "--force", "--options", "runtime", "--timestamp", "--sign",
                     identity, "--keychain", keychain, app])
                run(["codesign", "--verify", "--deep", "--strict", app])
                submission = private / "submission.zip"
                run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app, submission])
                auth = ["--keychain-profile", profile, "--keychain", keychain]
                submitted = json.loads(run(["xcrun", "notarytool", "submit", submission,
                                            *auth, "--output-format", "json"], timeout=600))
                receipt["submission_id"] = submitted["id"]
                # Persist the ID before waiting so a timed-out submission can
                # be inspected rather than blindly submitted again.
                (output / "signing-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
                print("Submitted to Apple; waiting for notarization.", flush=True)
                result = json.loads(run(["xcrun", "notarytool", "wait", submitted["id"], *auth,
                                         "--timeout", "30m", "--output-format", "json"],
                                        timeout=1850, status_json=True))
                receipt["notarization_status"] = result["status"]
                if result["status"] != "In Progress":
                    run(["xcrun", "notarytool", "log", submitted["id"], *auth, output / "notarization-log.json"])
                if result["status"] != "Accepted":
                    raise ValueError("Apple did not accept the submission; inspect notarization-log.json")
                run(["xcrun", "stapler", "staple", app])
                run(["xcrun", "stapler", "validate", app])
                run(["codesign", "--verify", "--deep", "--strict", app])
                run(["spctl", "--assess", "--type", "execute", app])
                receipt["signed_files"] = [{"file": str(relative), "sha256": digest(path)}
                                           for relative, path in files.items()]
                archive = output / f"VisualSubForge_{version}_{arch}_macos.zip"
                run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app, archive])
                receipt.update(archive=archive.name, sha256=digest(archive))
                (output / "SHA256SUMS").write_text(f"{receipt['sha256']}  {archive.name}\n")
                print(f"Signed, notarized and stapled: {archive.name}", flush=True)
            finally:
                if created:
                    try:
                        run(["security", "list-keychains", "-d", "user", "-s", *old_keychains])
                    finally:
                        run(["security", "delete-keychain", keychain])
        receipt["status"] = "passed"
    finally:
        (output / "signing-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    try:
        sign(parser.parse_args())
    except subprocess.TimeoutExpired:
        parser.exit(1, "Signing operation timed out; inspect the saved submission ID before retrying.\n")
    except subprocess.CalledProcessError:
        parser.exit(1, "Copying the unsigned app failed.\n")
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f"Signing failed: {error}\n")
