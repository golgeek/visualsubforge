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

Release workflows now capture `application-sources.tar.gz` alongside the native
artifacts. `scripts/application-sources.py` resolves the non-development Cargo
graph for each target and the production GUI/CLI features, preserves checksum-
verified published crates, and compares their extracted source against the
compiler inputs. Build dependencies and procedural macros are included and are
not mislabelled as necessarily linked runtime code. When a crate omits its
monorepo's license files, its checksum-verified `.cargo_vcs_info.json` selects the
exact additional upstream source/notices archive.
`libappindicator-sys` 0.9.0 also omits repository metadata; a version/revision-
specific mapping points to its upstream repository after comparing every source
file and the original manifest with that commit. Upgrades require another review.

The Vite build emits `frontend-inventory.json` and `THIRD-PARTY-NOTICES.txt` from
modules contributing to the production chunks. Vite/Rolldown are also retained
as generators of embedded runtime helpers. The collector checks bundled module
hashes against npm's integrity-verified source archives and records final web
asset hashes. Application source archives include tracked files only; local
planning documents are excluded. Installed packages carry the inventory and
notices; assembly receipts bind that inventory by SHA-256. Native inventories,
platform runtime coverage and the final artifact audit remain separate
work, so successful collection is not yet complete redistribution acceptance.

The application collector also preserves rustup's target-specific `rust-std`
binary component, its matching `rust-src` (including vendored libraries and
native unwind sources), notices, package identities and distribution manifest.
Every installed standard-library link input must match the manifest-checksummed
component; `Cargo.lock` alone does not identify this code. The archive includes
unused/build/test inputs conservatively and does not claim a per-symbol link map.
Local ARM64 validation verified all 59 standard-library files and 77 source
manifests; hosted captures for the four package architectures remain pending.

Windows also captures Microsoft.Web.WebView2 SDK 1.0.3800.47: all nine prebuilt
loader files in `webview2-com-sys` 0.39.1 match that exact SDK byte-for-byte.
The x64 MSVC build selects `WebView2LoaderStatic.lib`. Its Microsoft SDK
`LICENSE.txt` (BSD-3-Clause) and `NOTICE.txt` accompany the Rust binding's MIT
notices. The collector pins the SDK hash and rejects changed crate versions or
loader bytes. This preserves binary provenance; the SDK does not include the
loader implementation source. The separately installed Evergreen WebView2
Runtime and MSVC/Windows SDK runtime attribution remain separate inventory work.

Fresh macOS SDK builds preserve ANGLE's resolved Git source checkouts and file
hashes, including nested gclient dependencies. The cache key excludes older SDKs
without source archives. The Mac assembler now checks custom library/source
hashes against successful ANGLE/mpv build receipts and captures Homebrew sources
from each actual input keg's installed formula, receipt and SBOM. It preserves
resources and patches; missing local patches require a historical formula that
matches exactly apart from Homebrew's omitted bottle metadata. Moving upstream
recipes cannot substitute for installed ones.

The workflow publishes these inputs together in `native-sources.tar.gz`.
Native notices and an inventory enter the app before signing; the assembly
receipt binds the inventory hash and the signing job verifies the whole bundle.
Local collection covers 36 installed packages, including exact Git resources,
embedded/local patches and SPDX `LICENSES/` texts. Hosted source capture on both
architectures remains pending, as does the static/header-only contribution audit.

## Remaining release work

Use the [manual acceptance checklist](preview-acceptance.md) with the separately
downloadable synthetic fixture artifact. It specifies expected playback, editing,
resync and persistence results and the environment evidence testers should record.
The first hosted fixture upload is still pending.

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
CLI/resync checks with networking disabled. [Run 38191188205](https://github.com/golgeek/visualsubforge/actions/runs/38191188205)
passed production assembly, installation and those controls. Independent download
verification matched both artifact checksums, all 124 packaged native-file hashes,
the two custom source archives and all 201 source payloads referenced by 91 distro
source manifests. This first artifact predates the application source collector.
Its replacement stopped at missing `libappindicator-sys` provenance, now corrected
and verified by a full local Linux-target capture (360 Rust packages, four frontend
inputs). Hosted artifact binding and actual GUI/GPU/audio validation remain open.
A container CLI pass is not desktop installation acceptance.

The next workflow revision also runs the installed production GUI's existing
native smoke diagnostic in a second, unprivileged container with Xvfb, Mesa
software rendering and a PulseAudio null sink. It checks seeking, frame steps,
track selection, loop/replacement behavior and ordered shutdown, and records
observed library hashes. It requires both a successful exit and the diagnostic
PASS marker. This lane is configured but has not passed yet; even a pass cannot
establish visible pixels, audible output or physical GPU behavior.

The first hosted Linux run, [38190349153](https://github.com/golgeek/visualsubforge/actions/runs/38190349153),
passed the ELF relocation control and built libplacebo (13 tests passed, two GPU
tests skipped). mpv compiled and passed 39/40 tests, including the lifetime and
audio-policy controls. Its encode test failed because `libavdevice-dev` was
missing, so the `av://lavfi` input was unavailable. The workflow now installs
that dependency; replacement run 38190727555 passes the native build/tests and
compiled the production application. Assembly then exposed an `ldd` parser
omission: the dynamic loader has no SONAME arrow in that output. The parser now
retains it in the observed OS closure, with a regression control. No test was skipped
or relaxed to accommodate the failure.

### Windows candidate workflow

`Windows preview package` builds an x64 production GUI/CLI and a current-user
NSIS installer on Windows Server 2022. FFmpeg 8.1.2 is built from a checksum-pinned
source archive with an explicit external-dependency list. Built-in decoders,
demuxers and filters remain enabled; the unused Whisper/GGML runtime is excluded
to avoid its known unload hazard. The private DLL graph rejects GGML/Whisper,
MSYS emulation DLLs, missing dependencies and ambiguous basenames, including
delay imports. This policy still needs hosted execution evidence.

The installer places the analysis tools and native DLLs beside the application
and installs WebView2 if needed (initial installation may require networking).
MSVC code uses its static C runtime; other compiler runtimes must appear in the
private dependency graph. Package checks install into a new path, compare all
native hashes, run relocation/resync/input-preservation controls with a scrubbed
PATH and unload libmpv three times while checking the C++ terminate handler.
The unload probe now also records loaded module paths/hashes and rejects modules
outside the private package and Windows runtime. A subsequent check attempts the
installed production GUI's native smoke sequence and ordered exit, preserving
logs for the hosted runner's actual display/audio capabilities. Both new runtime
checks still require hosted execution.
The hosted runner contains development software; those checks do not establish
clean consumer-Windows, GPU, audio or oldest-OS acceptance. Windows signing,
complete source/notices and that acceptance remain open. The initial workflow
is not a claim that a Windows artifact has passed yet. First run 38190957247
stopped at a source-archive control whose expected LF bytes disagreed with
Windows text-mode CRLF output. The fixture now writes explicit CRLF bytes and
requires the archive to preserve them exactly on every platform.
Run 38191189515 then found that CLANG64 has no `lua52` package. The workflow now
uses the available LuaJIT package, which the pinned mpv build supports; this is
a build dependency correction, not a player-policy change.

The Windows source collector uses the assembled DLL paths to identify their
MSYS2 package owners. It verifies the downloaded binary/source archive signatures
with the runner's MSYS2 keyring, compares shipped DLL bytes with the signed
package, and checks the source PKGBUILD against the binary's `.BUILDINFO` hash.
It preserves source payloads, patches, recipes and notices, including notices
inside nested tarballs and pinned bare Git sources. Custom FFmpeg and patched
mpv sources/build inputs accompany them in `native-sources.tar.gz`; inventories
and notices enter the installer before bundling. Local controls and real LuaJIT/
libplacebo source examples pass; hosted execution of this collector is pending.
Static/header-only contributions and language/platform runtimes remain separate
inventory work, so package-owner coverage alone is not final license acceptance.

The collector now follows a reviewed set of embedded inputs from each signed
binary's `.BUILDINFO`: MinGW/compiler runtime inputs, libplacebo's generated/header
inputs and shaderc's glslang/SPIR-V inputs. It retains their historical versions,
including MSYS2 package epochs. Libdovi's nested `dolby_vision/Cargo.lock` supplies
its own registry sources/notices; the application's Cargo graph does not cover
those native Rust dependencies. All 84 entries were captured in a local source
check; hosted signature verification and full recipe coverage remain required.

Run 38198445762 built the production GUI/CLI and captured application sources,
then assembly identified `avicap32.dll` as absent from the explicit OS allowlist.
It is the Windows [AVICap system library](https://learn.microsoft.com/en-us/windows/win32/api/vfw/nf-vfw-capcreatecapturewindowa).
The allowlist now includes it; unknown libraries and compiler runtimes still
require a private packaged dependency. A successful installer is still pending.

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
