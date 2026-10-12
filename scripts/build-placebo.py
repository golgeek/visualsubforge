#!/usr/bin/env python3
"""Build the pinned shared libplacebo outside its source tree; retain its sources."""
import argparse
import json
from pathlib import Path
import platform
import subprocess
from native_evidence import digest
from source_archive import archive_source


COMMIT = "cee9b076f2c63104ccfd497fa79c39a867293ec4"  # v7.360.1
URL = "https://code.videolan.org/videolan/libplacebo.git"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--jobs", type=int, default=3)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 64:
        parser.error("--jobs must be between 1 and 64")
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    source, build, sdk = root / "source", root / "build", root / "sdk"
    commands = []
    receipt = {"status": "incomplete", "url": URL, "commit": COMMIT, "commands": commands,
               "host": {"system": platform.system(), "architecture": platform.machine()}}
    try:
        with (root / "commands.log").open("w") as log:
            def run(*command):
                command = list(map(str, command))
                commands.append(command)
                print("RUN", json.dumps(command), flush=True)
                log.write(json.dumps(command) + "\n")
                log.flush()
                subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)

            run("git", "clone", "--no-checkout", URL, source)
            run("git", "-C", source, "checkout", "--detach", COMMIT)
            run("git", "-C", source, "submodule", "update", "--init", "--recursive")
            receipt["submodules"] = subprocess.check_output(
                ["git", "-C", str(source), "submodule", "status", "--recursive"], text=True)
            archive_source(source, root / "source.tar.gz")
            receipt["source_sha256"] = digest(root / "source.tar.gz")
            # Upstream's GPU tests omit xxhash's include directory; its header
            # self-containment target similarly omits dav1d. Use the same
            # selected packages for all test/library targets, retaining tests.
            test_includes = []
            for dependency in ["libxxhash", "dav1d"]:
                selected = subprocess.run(["pkg-config", "--variable=includedir", dependency],
                                          text=True, capture_output=True)
                if selected.returncode == 0:
                    include = Path(selected.stdout.strip()).resolve(strict=True)
                    test_includes.append("-I" + str(include))
            header_options = [f"-Dc_args={json.dumps(test_includes)}",
                              f"-Dcpp_args={json.dumps(test_includes)}"] if test_includes else []
            # The Mac adapter renders exclusively through ANGLE/OpenGL. Do not
            # autodetect an unused Vulkan/MoltenVK backend from the build host.
            # Linux retains its existing Vulkan support and upstream tests.
            backend_options = ["-Dvulkan=disabled"] if platform.system() == "Darwin" else []
            run("meson", "setup", build, source, "--buildtype=release", "--wrap-mode=nodownload",
                f"--prefix={sdk}", "--libdir=lib", "-Ddefault_library=shared",
                "-Ddemos=false", "-Dtests=true", "-Dopengl=enabled", *backend_options, *header_options)
            receipt["dependencies"] = json.loads((build / "meson-info/intro-dependencies.json").read_text())
            receipt["build_options"] = json.loads((build / "meson-info/intro-buildoptions.json").read_text())
            # libplacebo embeds xxhash's inline header, so no linked DLL/dylib
            # identifies this dependency. Record the actual selected input.
            receipt["header_inputs"] = []
            if any(dep["name"] == "libxxhash" for dep in receipt["dependencies"]):
                include = subprocess.check_output(["pkg-config", "--variable=includedir", "libxxhash"],
                                                  text=True).strip()
                header = (Path(include) / "xxhash.h").resolve(strict=True)
                receipt["header_inputs"].append({"path": str(header), "sha256": digest(header)})
            run("meson", "compile", "-C", build, "-j", args.jobs)
            # Upstream's GPU tests may skip without a device; retain that fact.
            run("meson", "test", "-C", build, "--print-errorlogs", "-j", args.jobs)
            tests = [json.loads(line) for line in (build / "meson-logs/testlog.json").read_text().splitlines()]
            receipt["tests"] = [{key: test[key] for key in ("name", "result", "returncode")}
                                for test in tests]
            run("meson", "install", "-C", build)
            libraries = [path for path in (sdk / "lib").iterdir()
                         if path.is_file() and not path.is_symlink()
                         and (path.name.endswith(".dylib") or ".so" in path.name)]
            if len(libraries) != 1 or not libraries[0].name.startswith("libplacebo."):
                raise ValueError(f"Expected one installed shared libplacebo: {libraries}")
            receipt["binaries"] = [{"path": str(path), "sha256": digest(path)} for path in libraries]
            receipt["status"] = "passed"
    finally:
        (root / "build-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
