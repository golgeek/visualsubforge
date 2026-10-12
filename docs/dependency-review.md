# Preview dependency review

This review concerns the actual candidate artifacts below. A completed collector
means that its declared inputs were captured; it is not, by itself, a review of
all code embedded in a binary. Release publication and manual runtime acceptance
remain separate gates in [releases.md](releases.md).

## Artifact evidence

| Target | Candidate evidence | Independent verification | Remaining work |
| --- | --- | --- | --- |
| Ubuntu 24.04 x64 | [38204884997](https://github.com/golgeek/visualsubforge/actions/runs/38204884997), commit `f57e353` | Three download hashes; 124 native files; 120 private libraries observed in the installed GUI; application inventory; 1,828 native build-input evidence files and 3,262 installed notice records | Replacement 38206295864 captured the JPEG XL/SDL2 inputs and passed installation/CLI, but failed the GUI loop-wrap assertion. Playback investigation is open. |
| macOS ARM64 | [38205417832](https://github.com/golgeek/visualsubforge/actions/runs/38205417832), commit `6706d90`, `macos-unsigned-aarch64` | Three download hashes; 49 native files; all 1,609 inventoried app files; 1,325 native source-evidence files and 959 installed native notices | Signing/notarization waits for the Intel package job. |
| macOS Intel | Same matrix run, second attempt | Native compilation/tests passed in the first attempt; source capture subsequently lost network access | Complete source assembly, download audit and notarization. No successful package from this attempt yet. |
| Windows x64 | [38204616450](https://github.com/golgeek/visualsubforge/actions/runs/38204616450); replacement [38206642301](https://github.com/golgeek/visualsubforge/actions/runs/38206642301) | Production executables and PE closure built; 198 GUI/70 CLI linker inputs captured; both response hashes and 14 installed attribution texts verified | Replacement passed signed/tampered controls and Microsoft terms capture, then stopped at unsupported gettext lzip notices. Format support and a corruption control are added; full source collection and installer/runtime execution remain pending. |

The Ubuntu `.deb` SHA-256 is
`3935adebba2da88a4aaedc27f615bac5a01b18b1dcd14b947e96c8bb55d385b8`.
Its embedded Ubuntu build-input inventory SHA-256 is
`b2906220cb32a39c8fd15d0fe3770ce02ae5bb1d68853c58eb4d9ff6a796fb3f`.
The ARM Mac native inventory SHA-256 is
`a8c85c75b433e7b2066c72c578d4a17e37e0263476939ce911b49a0ab998c8e2`.
Signing changes Mac native file hashes; use the later signing receipt to verify
the distributed signed app, not the unsigned file hashes above.

## Source and notice coverage

Application inventories describe the target-specific production Cargo graph,
including build dependencies and procedural macros. They preserve registry
checksums, missing upstream license files, tracked VSF source, Cargo lockfiles,
the Rust distribution manifest, matching `rust-src` and target `rust-std` inputs.
The audited Linux archive contains 360 Cargo packages and 62 standard-library
files; the ARM Mac archive contains 282 packages and 59 standard-library files.
These counts include build inputs and are not claims that every crate is linked.

Both audited frontend inventories identify `@tauri-apps/api` 2.12.0
(Apache-2.0 OR MIT), Svelte 5.57.1 (MIT), Rolldown 1.2.11 (MIT) and Vite 8.3.1
(MIT). Rolldown/Vite contribute generated helpers. The inventories bind their
source archives and contributing modules to the built assets. The Rust graphs
also include MPL-2.0 packages (`cssparser`, `cssparser-macros`, `dtoa-short`,
`option-ext`, `selectors`); their source and notices accompany the archive.
VSF's original crates remain GPL-3.0-or-later.

Ubuntu private-library owners have matching distribution source packages,
including Debian patches and installed copyright files. The supplementary
build-input inventory binds original package bytes to their build checksums and
resolves exact historical versions, including rav1e/librsvg Rust inputs and
FFmpeg/shaderc/libplacebo/glslang/SPIR-V embedded inputs. Different builds used
different SPIR-V and Rust revisions; retaining today's development package alone
would not identify them. Reviewed Debian recipes additionally select Highway
headers for JPEG XL and Vulkan headers for SDL2. Their replacement source capture
passed, but its runtime failure prevents a final candidate audit. The supplementary collector verifies publisher HTTPS
provenance and hashes; it does not verify Debian developer PGP signatures or
reproduce the distributor's original build.

The ARM Mac graph contains 35 Homebrew packages plus custom mpv, ANGLE and
libplacebo builds. Installed recipes/SBOMs or verified legacy receipts identify
the source archives, resources and patches. Shaderc's own pinned glslang/SPIR-V
resources are included. Custom libplacebo preserves its pinned generated/header
inputs, and the inventory separately identifies the exact external `xxhash.h`.
The selected OpenGL build has no Vulkan-loader runtime dependency. Review of
these installed recipes found no further external static/header input beyond
those captured. This conclusion applies to this graph, not future upgrades.

One legacy metadata exception needs an explicit reading: uchardet 0.0.8's
installed formula has `license: null`. Its source archive SHA-256 is
`e97a60cfc00a1c147a674b097bb1422abd9fa78a2d9ce3f3fdcc2e78a34ac5f0`.
The headers of `src/uchardet.cpp`, `src/uchardet.h` and
`src/nsUniversalDetector.cpp` offer MPL-1.1, GPL-2.0-or-later or
LGPL-2.1-or-later. The README confirms these alternatives, and the complete
`COPYING` file contains all three license texts. Retain those original notices and source declarations;
the GPL alternative is the relevant selection for this GPL distribution.

Windows native collection follows actual MSYS2 DLL owners and reviewed
static/header/compiler inputs from signed package `.BUILDINFO` records. It must
also retain libdovi's own locked Rust dependencies. Custom FFmpeg excludes
Whisper/GGML and preserves its source, patch and explicit configure recipe.
Successful full collection against the current Windows package is still required.

## Platform components and source availability

Ubuntu GTK/WebKit, libc, compiler runtimes and display/audio drivers are supplied
through declared apt dependencies. Mac system frameworks/runtimes come from
macOS. They are not copied into VSF's private media-library directory. Compiler
and OS components must not be confused with bundled third-party media code.

Windows final-link evidence identifies MSVC 14.44.35207 and Windows SDK
10.0.26100.0. `libcmt.lib` contributes to both executables; `libvcruntime.lib`
also contributes to the GUI. The inventory hashes these selected inputs and
retains attribution/terms, without redistributing proprietary SDK library files.
Pinned Microsoft developer distribution terms remain reference evidence;
C runtime end-user terms accompany installed notices in the replacement build.

The WebView2 loader is another explicit boundary: all nine prebuilt files in
`webview2-com-sys` 0.39.1 were matched to Microsoft.Web.WebView2 SDK 1.0.3800.47.
The MSVC GUI uses the x64 `WebView2LoaderStatic.lib`. The archived SDK supplies
BSD-3-Clause license text and notices, **not loader implementation source**.
Its archive is binary provenance, not recovered corresponding source. The
Evergreen WebView2 Runtime is separately installed by the bootstrapper. Keep
this source-availability limit visible during final redistribution review;
do not describe the Windows archive as an entirely source-built native stack.

## Acceptance still required

Finish the pending table entries, review their actual versions and compare the
installed inventories/notices with the matching downloadable source archives.
Windows installation must pass the private DLL load/unload controls and packaged
GUI smoke test; Mac signatures, notarization receipts and final checksums must
pass their separate audit. Then update this review to the final candidates.

The Ubuntu installed GUI evidence uses Xvfb, Mesa llvmpipe and a PulseAudio null
sink. It establishes neither visible pixel correctness nor audible output.
All platforms still need the [manual acceptance checklist](preview-acceptance.md),
including clean consumer systems, real display/audio devices and claimed oldest
OS versions. No release has been published.
