# Preview release preparation

`v0.0.1-alpha` is in preparation and has not been published. Release builds are
to run on GitHub Actions, including native media dependencies. The repository
became public on 2026-10-11; standard hosted Actions jobs now run successfully.
The user confirmed on 2026-10-11 that publication must wait for packages on all
three platforms: macOS, Windows and Linux. Successful Mac validation artifacts
do not authorize a macOS-only release.

The macOS matrix has separate Apple Silicon and Intel validation artifacts.
Their audited minimums are macOS 14.0 and 15.0 respectively. The Intel Homebrew
OpenSSL 3.6.3 (libssl/libcrypto), xz 5.8.3, Vulkan loader 1.4.357.0 and pcre2
10.47_1 libraries require macOS 15. Lowering that floor requires rebuilding or
replacing those dependencies and repeating validation; setting the application's
deployment target alone is insufficient.
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
signing and notarization follow assembly. Publishing also requires the completed
license/source inventory and install/runtime checks below.
Apple bundle version fields use numeric `0.0.1`; the application/CLI, artifact
names and `VSFReleaseVersion` retain the full `0.0.1-alpha` version.

Original VSF code is GPL-3.0-or-later. Existing third-party patches retain their
licenses. Distributable artifacts need notices and the corresponding source and
build instructions for the actual bundled dependencies; a package-manager
license name alone is not that inventory.

## Remaining release work

### Linux candidate workflow

`Linux preview package` is a manually dispatched Ubuntu 24.04 amd64 workflow.
It builds pinned libplacebo and the patched mpv candidate, then production GUI
and CLI binaries. The `.deb` installs private media binaries/libraries in
`/usr/lib/visualsubforge`; `/usr/bin/visualsubforge` and `/usr/bin/vsf` launch
them. Apt resolves GTK/WebKit, display/audio libraries and ordinary OS runtimes.
Users install the downloaded package with `sudo apt install ./<package>.deb`.

Assembly audits the ELF dependency closure, records hashes, and downloads exact
Ubuntu source packages (including distribution patches) for bundled libraries.
The workflow installs the package in a fresh Ubuntu container and runs relocated
CLI/resync checks with networking disabled. This is the first hosted validation
of the Linux packaging implementation; no successful package is claimed yet.
Complete Rust/frontend provenance and actual GUI/GPU/audio validation are still
required. A container CLI pass is not desktop installation acceptance.

### Publication gates

These are implementation and validation tasks, not unanswered credential or
license-selection questions. The original-code license and Apple credentials
are already settled.

| Work | Required evidence before publication |
| --- | --- |
| Dependency inventory | Identify the actual bundled native, Rust and frontend dependencies; retain their versions, licenses, notices, matching sources, patches and build recipes alongside artifact hashes. Audit the package contents, not only the development dependency list. |
| Windows and Linux packages | Build the native runtime and analysis tools on GitHub Actions, package their required dependencies, and declare the supported OS/architecture and installation flow. Current shell/CLI CI is not package support. |
| Installation and runtime | Install the actual artifacts on clean supported environments, then check offline startup, media playback/seeking/audio, subtitle preview, waveform/scene analysis, GUI/CLI resync, save/reopen and shutdown. Record which tests use VMs and which use real GPU/audio devices. |
| Publication | Require green checks for the release revision and archive the package/source/checksum evidence for all three platforms before publishing the prerelease. |

## Validation so far (2026-10-11)

- All ten [application checks](https://github.com/golgeek/visualsubforge/actions/runs/38177639367)
  and all six [native build contracts](https://github.com/golgeek/visualsubforge/actions/runs/38177639286)
  pass at `616a1921`, including all 32 Chromium/WebKit browser cases. These runs
  initially could not start because of account billing limits; their retries
  completed after the repository became public. No additional code fix or
  weakened check was needed for these retries.
- Hosted native contracts at [`a222eda`](https://github.com/golgeek/visualsubforge/actions/runs/38171415744): all six jobs passed, including all three
  pinned mpv source variants and compilation of each native adapter. This is not
  GPU/audio or packaged-app acceptance.
- All ten [application CI jobs at `a222eda`](https://github.com/golgeek/visualsubforge/actions/runs/38171415641)
  pass, including the default-app and headless lanes on macOS, Windows and
  Ubuntu, and all 30 modeled browser checks. Earlier browser-coordinate and
  Windows fixture-path failures are corrected; the earlier Mac artifact-upload
  DNS failure did not recur in this run.
- At [`37b661b`](https://github.com/golgeek/visualsubforge/actions/runs/38173751167),
  nine jobs pass but WebKit's project fixture times out (29/30 browser cases
  pass). A completed automatic scene scan replaces the transient project-restored
  notice before the test observes it. A controlled delayed-observation case
  reproduces the failure; the fixture now waits for project attachment and an
  idle workspace, and holds fresh scan completion while checking legacy marker
  suppression. All six project cases and the full 32-case browser suite pass
  locally in Chromium/WebKit, including the new control; type checking, lint
  and formatting also pass. Hosted validation of the fix now passes in the
  32-case browser job linked above.
- The first hosted ARM64 package build stopped before compilation: the default
  Xcode 15.4 SDK was below pinned ANGLE's SDK requirement. Packaging now explicitly
  selects the runner's Xcode 16.2 (macOS 15.2 SDK), retaining the macOS 14 deployment
  target. The [subsequent ARM64 build](https://github.com/golgeek/visualsubforge/actions/runs/38171420846)
  passes assembly, packaged CLI checks, Developer ID signing, Apple notarization,
  stapling and Gatekeeper assessment. Its audited effective minimum is macOS
  14.0. This does not establish clean-install or native playback acceptance.
- The [first Intel build](https://github.com/golgeek/visualsubforge/actions/runs/38168830921)
  compiled ANGLE, mpv, the GUI and CLI but failed assembly: signing the main
  executable first implicitly tried to seal a bundle with unsigned helpers.
  Assembly now signs the nested binaries first and the completed app last.
  A real macOS regression forces unsigned inputs, reproduces the old failure
  on Apple Silicon too, and passes after the fix; all eight release tests pass
  locally. The [corrected Intel run](https://github.com/golgeek/visualsubforge/actions/runs/38173750414)
  now passes assembly, packaged CLI checks, signing, Apple notarization,
  stapling and Gatekeeper assessment. Its signing job was initially blocked by
  billing and succeeded when retried using the completed package.
- The downloaded Intel ZIP checksum and all 50 signed native-file hashes match
  the signing receipt. Local signature, stapling and Gatekeeper checks pass,
  as does the packaged CLI test on Apple Silicon through Rosetta. This does not
  establish physical Intel, clean-install, GPU or audio acceptance. Its audited
  macOS minimum is 15.0, as described above.
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

Hosted ARM64 and Intel signing/notarization pass; their signing receipts and
Apple's acceptance logs are attached to the package runs above. Signed validation
artifacts remain separate from a published release: the dependency
source/license inventory and install/runtime checks still need completion.
Credentials and exported private keys must never enter source, logs or artifacts.

Windows Authenticode signing is recommended but is not required on ordinary
Windows configurations; SmartScreen reputation warnings can remain for a new
signed release. Linux direct downloads have no centralized notarization gate;
checksums and any package/repository signatures must accompany the chosen
distribution flow. See the official [macOS](https://v2.tauri.app/distribute/sign/macos/),
[Windows](https://v2.tauri.app/distribute/sign/windows/) and
[Linux](https://v2.tauri.app/distribute/sign/linux/) signing guides.
