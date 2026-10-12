# Preview release preparation

`v0.0.1-alpha` is in preparation and has not been published. Release builds are
to run on GitHub Actions, including native media dependencies. The repository
became public on 2026-10-11; standard hosted Actions jobs now run successfully.
The user confirmed on 2026-10-11 that publication must wait for packages on all
three platforms: macOS, Windows and Linux. Successful Mac validation artifacts
do not authorize a macOS-only release.

The macOS matrix has separate Apple Silicon and Intel validation artifacts.
Their audited minimums are macOS 14.0 and 15.0 respectively. The Intel Homebrew
OpenSSL 3.6.3 (libssl/libcrypto), xz 5.8.3 and pcre2 10.47_1 libraries require
macOS 15. The current OpenGL-only package no longer includes Vulkan loader. Lowering that floor requires rebuilding or
replacing those dependencies and repeating validation; setting the application's
deployment target alone is insufficient.
An advertised minimum and an OS version actually tested are separate evidence.
Windows and Linux packaging/validation are also pending. None of these targets
is accepted merely because an application or compiler lane passes.

The current Mac candidates from [run 38205417832](https://github.com/golgeek/visualsubforge/actions/runs/38205417832)
have matching source inventories, accepted notarization and independently
verified signed downloads for both architectures. See the [dependency review](dependency-review.md)
for final archive hashes, download links and audit scope. Windows run 38209801548
failed source-notice collection on two deliberately non-archive Rust test fixtures;
the focused correction has passed local checks and requires a replacement build.
Linux run 38208853449 passed its package lane,
but repeated playback diagnostics reproduced a failure; neither target is
accepted from compilation alone. Earlier run notes below retain their original evidence scope.

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
The [artifact dependency review](dependency-review.md) records the independently
audited candidates, source coverage and explicit vendor-source limitations.

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
Independent downloaded-archive audits verified 59 standard-library files per Mac
architecture and 62 for Linux. Windows application capture has run, but its final
package/source comparison remains pending.

Windows also captures Microsoft.Web.WebView2 SDK 1.0.3800.47: all nine prebuilt
loader files in `webview2-com-sys` 0.39.1 match that exact SDK byte-for-byte.
The x64 MSVC build selects `WebView2LoaderStatic.lib`. Its Microsoft SDK
`LICENSE.txt` (BSD-3-Clause) and `NOTICE.txt` accompany the Rust binding's MIT
notices. The collector pins the SDK hash and rejects changed crate versions or
loader bytes. This preserves binary provenance; the SDK does not include the
loader implementation source. The separately installed Evergreen WebView2
Runtime is outside that loader archive. MSVC/Windows SDK attribution is captured
separately as described below.
The Windows package build now requests MSVC's
[`/LINKREPROFULLPATHRSP`](https://learn.microsoft.com/en-us/cpp/build/reference/link-repro-full-path-rsp?view=msvc-170)
diagnostic for both final executables. The collector hashes the selected compiler
and SDK inputs, records versions and installed license texts, and binds the
inventory to the packaged executable hashes. Compiler/SDK library payloads are
not copied into the source archive. Run 38204616450 captured and hashed 198 GUI
and 70 CLI inputs, including `libcmt.lib` in both executables and `libvcruntime.lib`
in the GUI. All 14 installed attribution texts and both linker response files
passed an independent download hash audit. The collector also retains pinned
Microsoft publisher terms: Visual Studio 2022's developer distribution terms
remain reference evidence, while the C runtime end-user terms enter installed
notices. These are distinct from VSF's GPL license. Run 38206642301 captured both
publisher terms; independent verification checked their source documents,
derived text and all 15 attribution records. Their inclusion in the completed
Windows installer still requires verification.

Fresh macOS SDK builds preserve ANGLE's resolved Git source checkouts and file
hashes, including nested gclient dependencies. The cache key excludes older SDKs
without source archives. The Mac assembler now checks custom library/source
hashes against successful ANGLE/mpv build receipts and captures Homebrew sources
from each actual input keg's installed formula, receipt and SBOM. It preserves
resources and patches; missing local patches require a historical formula that
matches exactly apart from Homebrew's omitted bottle metadata. Moving upstream
recipes cannot substitute for installed ones.
Older bottles without an SBOM require an installed stable core-bottle receipt
and an exact historical recipe match. Their inventory explicitly records that
different provenance; it does not invent an SBOM or a bottle-signature check.
The missing SBOM in the runner's uchardet 0.0.8 bottle exposed this case; a local
copy without an SBOM passed collection and source-checksum verification.

The workflow publishes these inputs together in `native-sources.tar.gz`.
Native notices and an inventory enter the app before signing; the assembly
receipt binds the inventory hash and the signing job verifies the whole bundle.
The current hosted Mac archives cover 35 Homebrew packages per architecture,
including exact Git resources, embedded/local patches and SPDX `LICENSES/`
texts, plus the custom native builds. Independent source/notice hash audits and
review of their actual recipes have passed; see the dependency review for scope.

macOS now uses the same pinned libplacebo source-build recipe as Linux. Homebrew's
libplacebo bottle metadata omitted the exact `fast_float` build input; an archive
of today's headers could not establish its source provenance. The shared recipe
instead captures libplacebo's pinned submodules, including `fast_float`, Vulkan
headers and code generators, with Meson options/dependencies and the installed
library hash. The Mac assembler requires the supplied custom library to appear
in the actual bundle and checks it against the successful build receipt.
Both hosted architectures now build and pass their packaged CLI controls.
Native playback and manual device acceptance remain separate gates.
The build receipt also identifies the actual inline `xxhash.h` input, which
does not appear in the dynamic-library graph. Both assemblers verify that header
has not changed and capture its owning package's sources/notices.
The shared recipe supplies the selected xxhash and dav1d include directories to
upstream test targets too: their Meson dependency lists omitted them, causing
hosted Mac builds to fail on those headers. The corrected recipe passes locally
with all tests enabled. The next hosted run exposed a separate Vulkan test abort:
MoltenVK calls an unsupported Metal selector on the runner's paravirtual GPU.
The Mac library now explicitly selects the adapter's OpenGL backend and disables
the unused Vulkan backend for the shipped build, rather than autodetecting it
from Homebrew. Linux retains its existing backend selection. The resulting local
Mac build passes 14 tests with one explicit OpenGL device skip. This does not
validate ANGLE playback or claim virtual-GPU support. Replacement run 38205417832
passes both hosted build lanes and signing/notarization.
The pinned mpv Cocoa player also referenced its Vulkan-only `MetalLayer` without
the corresponding compile guard. Both hosted architectures exposed that error
once Vulkan was disabled. The checked render patch now supplies `HAVE_VULKAN`
around that reference; a local OpenGL-only candidate build passes all 39 upstream
tests, including the required lifetime and audio controls.

## Remaining release work

Use the [manual acceptance checklist](preview-acceptance.md) with the separately
downloadable synthetic fixture artifact. It specifies expected playback, editing,
resync and persistence results and the environment evidence testers should record.
The fixtures are available in [run 38198449535](https://github.com/golgeek/visualsubforge/actions/runs/38198449535)
as `preview-acceptance-fixtures`; all 17 file hashes were independently checked.

### Linux candidate workflow

The latest [candidate 38208853449](https://github.com/golgeek/visualsubforge/actions/runs/38208853449)
is downloadable as [linux-preview-amd64](https://github.com/golgeek/visualsubforge/actions/runs/38208853449/artifacts/11728388564),
including its native/application source archives. Its `.deb` SHA-256 is
`4ed408d688ff09fa0a2f14db6927b58aea36a8addc85bef8d3c785f539865849`.
Independent download verification checked three artifact hashes, 124 native
files, 120 observed private GUI libraries, 1,063 application-source files and
1,850 native build-input evidence files. All 3,271 installed build-input notice
records match their inventory. Clean-container installation and offline CLI
checks pass.

The package lane's GUI smoke passed, but [ten-trial diagnostics 38210869329](https://github.com/golgeek/visualsubforge/actions/runs/38210869329)
reproduced the loop failure twice against the exact same candidate and fixture.
The two failures remained at position 1.04 until about 1.1 seconds after unpause;
subsequent wraps were about 0.5 seconds apart. This is measured startup behavior,
not proof of its cause. Further diagnostics retain PulseAudio server logs and
compare its default null sink with the explicit `norewinds=true` control.
[PulseAudio 16.1's implementation](https://github.com/pulseaudio/pulseaudio/blob/v16.1/src/modules/module-null-sink.c)
uses different maximum buffer lengths for these configurations. The default
package check and two-wrap/two-second assertion remain unchanged. A pass with a
different virtual sink cannot by itself establish a fix or real-device support.
The logged [default comparison 38211883635](https://github.com/golgeek/visualsubforge/actions/runs/38211883635)
and [no-rewinds comparison 38211885685](https://github.com/golgeek/visualsubforge/actions/runs/38211885685)
both pass ten trials. First progress beyond 1.04 seconds occurs 0.560–0.937
seconds after unpause with the default sink and 0.064–0.097 seconds with the
control. Every trial's executable, fixture and 120 private-library hashes match
the candidate. These passes do not erase the earlier reproduced failure.
The `ready-default` diagnostic retains rewind behavior and uses a separate silent
client to establish a bounded virtual-device clock before launching VSF. Its
readiness observations and cleanup are retained; this comparison is still pending.

Earlier candidate evidence:

[Run 38204884997](https://github.com/golgeek/visualsubforge/actions/runs/38204884997)
passes actual `.deb` installation, offline CLI relocation/resync/preservation
checks and the installed GUI's native smoke/ordered shutdown. The GUI environment
was an Ubuntu 24.04 container using Xvfb, Mesa llvmpipe software rendering and a
PulseAudio null sink. Download verification matched all three artifact hashes,
124 native package files and 120 observed private libraries to their receipts;
the embedded application inventory also matches its source archive. Package SHA256:
`3935adebba2da88a4aaedc27f615bac5a01b18b1dcd14b947e96c8bb55d385b8`.
This candidate includes native Rust and reviewed shader/header build-input
sources. It does not establish
real GPU/audio, interactive desktop or oldest-supported-OS acceptance.

The native build-input collector binds the
pre-relocation rav1e library to Ubuntu's original `.deb`, verifies that binary
and its buildinfo against the original build's checksums, and resolves exact
`librust-*`, compiler and standard-library source versions from that buildinfo.
It retains Ubuntu/Debian source descriptors, upstream archives, patches and
notices, including a conservative superset of test/build dependencies. Local
capture and a separate hash audit pass for 307 binary inputs, 276 source packages,
1,692 evidence files and 2,141 notice records. The downloaded hosted source archive
and installed notices pass the same hash audit; the embedded inventory matches
the assembly receipt and source archive.
This is publisher HTTPS provenance with checksum verification, not a claim to
have verified the Debian developers' source signatures or reproduced their build.
Librsvg's vendored crate payload is already present in Debian's source archive;
the collector also follows its exact compiler/std build. This differs from
rav1e's compiler revision in the audited package. The next package also follows
reviewed C++ static/header inputs for FFmpeg, shaderc and the older distro
libplacebo, recursively reading glslang and SPIRV Tools' own build records. Exact
versions differ between those builds, so the capture retains both SPIRV versions.
Local capture and an independent audit pass for five original builds, nine source
packages, 79 evidence files and 28 notices. A separate FFmpeg capture binds its two
executables to the original Ubuntu binary and confirms its exact glslang/SPIRV/
Vulkan-header inputs. Legacy Debian `.diff.gz` payloads are retained with their
packaging copyright text. The renamed `ubuntu-build-inputs.json` inventory is
bound to the assembly receipt. Run 38204884997 passed hosted integration;
independent download verification checked 16 original builds, 318 exact binary
inputs, 286 source packages, 1,828 evidence files and 3,262 installed notice records.
Review of the archived Debian recipes also identified JPEG XL's Highway inline
code and SDL2's Vulkan headers. Their original build records select Highway
1.0.7-8.1build1 and Vulkan loader/headers 1.3.275.0-1build1. Local capture and a
separate audit verify 22 evidence files and nine notices for these two inputs;
their capture passed in run 38206295864, as did clean installation and offline
CLI checks. Its GUI selection-loop assertion failed. Diagnostic run 38208265216
then passed the unchanged checks in ten fresh containers using the previous
candidate; this does not explain the failure. The next build records timed
playback samples and retains exact candidate/source artifacts before runtime
checks so a failing binary can be reproduced without rebuilding it. The existing
two-wrap/two-second requirement is unchanged.
An independent comparison of the failing run's fixture with the ten-trial
control verified identical contents and timestamps for all 132 encoded packets
and all decoded video/audio frames. The container-file hashes differ, but this
does not represent different stimulus or timing. Local Mac mpv controls with
video retained and null video/audio outputs observed three wraps in two seconds
for both the candidate and render-only control; that environment does not
reproduce the Linux PulseAudio case or clear its failure.

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
delay imports. Run 38204616450 passed this assembly policy against the actual
production binaries; installed runtime execution remains pending.

The installer places the analysis tools and native DLLs beside the application
and installs WebView2 if needed (initial installation may require networking).
MSVC code uses its static C runtime; other compiler runtimes must appear in the
private dependency graph. Package checks install into a new path, compare all
native hashes, run relocation/resync/input-preservation controls with a scrubbed
PATH and unload libmpv three times while checking the C++ terminate handler.
The next installation check also compares every assembled resource, including
inventories and license notices, with its installed copy. The unload probe
records loaded module paths/hashes and rejects modules
outside the private package and Windows runtime. A subsequent check attempts the
installed production GUI's native smoke sequence and ordered exit, preserving
logs for the hosted runner's actual display/audio capabilities. Future runs retain
the exact installer, portable archive and sources before these runtime checks;
an artifact from a failed run remains an unaccepted diagnostic candidate. Both new runtime
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
with GnuPG and the runner's MSYS2 keyring, requiring both a valid signature and
full/ultimate signer trust, compares shipped DLL bytes with the signed
package, and checks the source PKGBUILD against the binary's `.BUILDINFO` hash.
It preserves source payloads, patches, recipes and notices, including notices
inside nested tarballs and pinned bare Git sources. Custom FFmpeg and patched
mpv sources/build inputs accompany them in `native-sources.tar.gz`; inventories
and notices enter the installer before bundling. Local controls and real LuaJIT/
libplacebo source examples pass; hosted execution of this collector is pending.
Static/header-only contributions and language/platform runtimes remain separate
inventory work, so package-owner coverage alone is not final license acceptance.
Run 38206199337 isolated an upstream pacman 6.1.0-25 wrapper inconsistency:
its verifier requires `lock-never`, but its initializer no longer writes that
setting. The collector now calls GnuPG directly against the same keyring and
retains locking. A real signed/tampered package control runs before compilation;
run 38206642301 passed that control and verified the first eight source packages.
It then found an unsupported nested format in GNU gettext 1.0: `.tar.lz`.
The collector now uses MSYS2 lzip to verify/decode that payload before scanning
notices, with an early real compressed/truncated archive control. Local capture
finds 17 gettext notices; four runtime license files independently match system
libarchive extraction. Full replacement source collection remains required.

Run 38209801548 reached Rust 1.97 source capture and found two Insta binary
snapshots whose `.tar.gz` filenames deliberately contain the four bytes `test`.
The collector now recognizes only the reviewed version/path and exact content,
records their hashes as non-archive fixtures, and retains the complete source
archive. Changed fixtures and other unreadable nested archives still fail.
The exact Rust source archive yields 3,800 notices locally, with every recorded
notice hash checked and both fixture exceptions recorded. The 48 release controls
pass with one Linux-only skip on macOS. Hosted full collection and installer
validation remain required.

The collector now follows a reviewed set of embedded inputs from each signed
binary's `.BUILDINFO`: MinGW/compiler runtime inputs, libplacebo's generated/header
inputs, shaderc's glslang/SPIR-V inputs and the Vulkan loader's exact headers. It retains their historical versions,
including MSYS2 package epochs. Libdovi's nested `dolby_vision/Cargo.lock` supplies
its own registry sources/notices; the application's Cargo graph does not cover
those native Rust dependencies. The older libdovi 3.3.2 lockfile had 84 entries;
the currently selected 3.4.0 has 90. Its local capture passes after recovering
notices for two legacy winapi import-library crates: every published payload
(including prebuilt archives) matches the reviewed upstream release commit.
The fallback requires the exact crate checksum and retains the matching source,
definitions, Cargo build scripts and license texts. Regeneration of those
prebuilt import libraries was not tested. Hosted integration remains
required; unrelated crates without provenance still fail.

Run 38198445762 built the production GUI/CLI and captured application sources,
then assembly identified `avicap32.dll` as absent from the explicit OS allowlist.
It is the Windows [AVICap system library](https://learn.microsoft.com/en-us/windows/win32/api/vfw/nf-vfw-capcreatecapturewindowa).
The allowlist now includes it; unknown libraries and compiler runtimes still
require a private packaged dependency. A successful installer is still pending.
Run 38200157752 next identified `bcryptprimitives.dll`, the documented Windows
[ProcessPrng system library](https://learn.microsoft.com/en-us/windows/win32/seccng/processprng).
The explicit list now includes it too. An unsuccessful graph audit reports all
unresolved imports together and still refuses assembly.

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
