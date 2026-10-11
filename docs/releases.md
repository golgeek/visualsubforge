# Preview release preparation

`v0.0.1-alpha` is in preparation and has not been published. Release builds are
to run on GitHub Actions, including native media dependencies. The repository
remains private; releases will initially be available to its collaborators.

The intended macOS matrix has separate Apple Silicon and Intel artifacts. Aim
for macOS 14 compatibility and audit every included Mach-O deployment target;
the effective minimum cannot be lower than any bundled dependency's minimum.
An advertised minimum and an OS version actually tested are separate evidence.
Windows and Linux packaging/validation are also pending. None of these targets
is accepted merely because an application or compiler lane passes.

## Shared package layout

Build desktop and CLI with `bundled-tools`. Both find `ffmpeg` and `ffprobe`
beside their executable, without PATH, Homebrew or `VSF_FFMPEG`/`VSF_FFPROBE`
fallback. Missing tools produce an installation error from the existing analysis
worker. Development builds retain their explicit overrides and system discovery.
The subtitle domain and IPC boundary do not own this discovery.

On macOS the four executables live in `Contents/MacOS`; their private dynamic
libraries live in `Contents/Frameworks`. `scripts/package-macos.py` copies inputs
into a new app, rejects missing/ambiguous libraries, rewrites links to bundle
paths, audits architecture/deployment targets, and applies an ad-hoc signature
for validation. It never overwrites an SDK or an earlier artifact. Developer ID
signing and notarization must follow assembly and the license/source inventory.
Apple bundle version fields use numeric `0.0.1`; the application/CLI, artifact
names and `VSFReleaseVersion` retain the full `0.0.1-alpha` version.

Original VSF code is GPL-3.0-or-later. Existing third-party patches retain their
licenses. Distributable artifacts need notices and the corresponding source and
build instructions for the actual bundled dependencies; a package-manager
license name alone is not that inventory.

## Validation so far (2026-10-11)

- Hosted native contracts at `ce43559` and `a565564`: all six jobs passed, including all three
  pinned mpv source variants and compilation of each native adapter. This is not
  GPU/audio or packaged-app acceptance.
- The first hosted default-app workflow passes on macOS, Windows and Ubuntu at
  `ce43559`. At `a565564`, the corrected browser fixture passes all 30 checks;
  Windows found a new tool-discovery test directory with a trailing space, now
  corrected, and the Mac artifact upload hit a storage DNS failure after its
  application tests passed. Neither failure is treated as a green run.
- The first hosted ARM64 package build stopped before compilation: the default
  Xcode 15.4 SDK was below pinned ANGLE's SDK requirement. Packaging now explicitly
  selects the runner's Xcode 16.2 (macOS 15.2 SDK), retaining the macOS 14 deployment
  target. Intel's first native dependency build is still in progress.
- Local `cargo test --locked --workspace` and default/packaged strict Clippy pass.
  `cargo test --locked -p media-analysis --features bundled-tools --lib tools::tests`
  exercises relocated tool paths and incomplete-installation failure.
- A local ARM64 package experiment contains 50 native files, including analysis
  tools. Its actual dependencies require macOS 27; it is not the intended older
  macOS release artifact.
- `python3 tests/release/packaged-cli.py --bundle <app> --bin Contents/MacOS
  --output <new-directory>` passes against that experiment: offset/speed/ad-cut
  retiming, Unicode/text preservation, unchanged inputs, retry protection and
  missing-tool failure. The test copies the package into a path containing
  spaces, clears developer library paths and poisons development tool overrides.
  It does not simulate a clean OS installation or test native playback.

Source SDKs for the local experiment were the recorded thirteen-patch mpv
candidate and pinned ANGLE/Metal runtime. This does not promote those experiments
to general device acceptance; the [native gates](native-runtime.md#remaining-adoption-gates)
remain applicable.

## Signing setup

macOS direct distribution uses a **Developer ID Application** certificate and
Apple notarization, rather than App Store review. Follow the
[macOS signing setup](macos-signing.md) for certificate creation, local Keychain
storage, an encrypted export and the exact GitHub Actions secrets. A local
`notarytool` profile cannot be used directly by hosted runners. The six secret
names are verified on GitHub, and the user configured local profile
`visualsubforge-notary`. No private credential values were read for that check.

The manual workflow accepts `architecture` (`both`, `aarch64`, `x86_64`) and
`sign` (default true). Its separate signing job runs on `main` after packaging,
with secrets scoped only to the signing step. It validates the assembly hashes,
uses a temporary Keychain, signs all native code with hardened runtime and a
timestamp, submits to Apple, requires acceptance, staples and verifies the app,
then archives it again. A submission ID is saved before waiting so a timeout
can be investigated without blindly resubmitting. The original unsigned app
is preserved. Assembly and signing receipts live outside the app and describe
the final hashes for their respective stages. The assembly receipt also covers
web assets/resources. The internal native inventory records source hashes;
embedding the final executable hash in its own signed resources would create a
hash cycle, since outer app signing changes that executable.

Signing integration is implemented but hosted acceptance is pending. Signed
validation artifacts remain separate from a published release: the dependency
source/license inventory and install/runtime checks still need completion.
Credentials and exported private keys must never enter source, logs or artifacts.

Windows Authenticode signing is recommended but is not required on ordinary
Windows configurations; SmartScreen reputation warnings can remain for a new
signed release. Linux direct downloads have no centralized notarization gate;
checksums and any package/repository signatures must accompany the chosen
distribution flow. See the official [macOS](https://v2.tauri.app/distribute/sign/macos/),
[Windows](https://v2.tauri.app/distribute/sign/windows/) and
[Linux](https://v2.tauri.app/distribute/sign/linux/) signing guides.
