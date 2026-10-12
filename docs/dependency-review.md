# Preview dependency review

This review concerns the actual candidate artifacts below. A completed collector
means that its declared inputs were captured; it is not, by itself, a review of
all code embedded in a binary. Release publication and manual runtime acceptance
remain separate gates in [releases.md](releases.md).

## Artifact evidence

| Target | Candidate evidence | Independent verification | Remaining work |
| --- | --- | --- | --- |
| Ubuntu 24.04 x64 | [38208853449](https://github.com/golgeek/visualsubforge/actions/runs/38208853449), commit `3c13449` | Three download hashes; 124 native files; 120 observed private GUI libraries; 1,063 application-source files; 1,850 native build-input evidence files and 3,271 installed notices | Installation/offline CLI passed. Cold null-sink diagnostics failed in 2/10 trials; [38212585168](https://github.com/golgeek/visualsubforge/actions/runs/38212585168) independently measured sink startup delay and passed 10/10 with sink readiness checked before VSF. Physical-device acceptance remains open. |
| macOS ARM64 | [38205417832](https://github.com/golgeek/visualsubforge/actions/runs/38205417832), commit `6706d90`, `macos-unsigned-aarch64` | Three download hashes; 49 native files; all 1,609 inventoried app files; 1,325 native source-evidence files and 959 installed native notices | Signed/notarized artifact verified separately; manual device/oldest-OS acceptance remains open. |
| macOS Intel | Same matrix run, second attempt; commit `6706d90` | Three download hashes; 49 native files; all 1,609 inventoried app files; 1,324 native source-evidence files and 959 installed notices; signed/notarized archive independently verified | Manual device/oldest-OS acceptance remains open. |
| Windows x64 | [38204616450](https://github.com/golgeek/visualsubforge/actions/runs/38204616450); platform attribution [38206642301](https://github.com/golgeek/visualsubforge/actions/runs/38206642301) | Production executables and PE closure built; 198 GUI/70 CLI linker inputs captured; both response hashes, 15 attribution records and two publisher terms verified | [38221142186](https://github.com/golgeek/visualsubforge/actions/runs/38221142186) passes the complete native collection and source-built installer-helper inventory. NSIS then failed opening a long notice path; compact notice staging, final installer/runtime validation and independent archive audits remain pending. |

The Ubuntu `.deb` SHA-256 is
`4ed408d688ff09fa0a2f14db6927b58aea36a8addc85bef8d3c785f539865849`.
Its embedded Ubuntu build-input inventory SHA-256 is
`7bbd4cbd940effc8e355ca4c547101d008c03c638d6e3c8be88a74752bd7b166`.
The ARM Mac native inventory SHA-256 is
`a8c85c75b433e7b2066c72c578d4a17e37e0263476939ce911b49a0ab998c8e2`.
Signing changes Mac native file hashes; use the later signing receipt to verify
the distributed signed app, not the unsigned file hashes above.

The signed Mac downloads are from [run 38205417832](https://github.com/golgeek/visualsubforge/actions/runs/38205417832):

| Architecture | Signed app artifact | Final ZIP SHA-256 |
| --- | --- | --- |
| Apple Silicon, macOS 14+ build floor | [macos-notarized-aarch64](https://github.com/golgeek/visualsubforge/actions/runs/38205417832/artifacts/11727984565) | `91a6a7910387316fb6760095606c8d82dce098841f70b197c9545332061c3af2` |
| Intel, macOS 15+ build floor | [macos-notarized-x86_64](https://github.com/golgeek/visualsubforge/actions/runs/38205417832/artifacts/11728236399) | `c2869dc0a02aa270238e86c8d82df7f807d8288e9d65841bf75fb640fb19cd5c` |

The corresponding `macos-unsigned-*` artifacts contain the matching native and
application source archives. Independent download checks verified each final
ZIP, all 49 signed native hashes, and 1,558 unchanged inventoried resources per
architecture against its unsigned assembly. Apple accepted both submissions;
local `codesign --verify --deep --strict`, `xcrun stapler validate` and
`spctl --assess --type execute` passed for both extracted apps. These are
signature/distribution checks on the development Mac, not playback or oldest-OS
tests. The Intel native inventory SHA-256 is
`ca5dc33e853293bacd5766a4787b54d2d4f14e571f0cbda1254755234ab4f036`.

## Source and notice coverage

Application inventories describe the target-specific production Cargo graph,
including build dependencies and procedural macros. They preserve registry
checksums, missing upstream license files, tracked VSF source, Cargo lockfiles,
the Rust distribution manifest, matching `rust-src` and target `rust-std` inputs.
The audited Linux archive contains 360 Cargo packages and 62 standard-library
files; each Mac archive contains 282 packages and 59 standard-library files.
Independent checks verified all 1,063 application-source/notice files on Linux
and 889 per Mac archive, including published crate hashes against the archived
Cargo.lock, compiler component hashes, frontend sources and original notices.
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
headers for JPEG XL and Vulkan headers for SDL2. The replacement inventory audit
verifies 18 original builds, 320 exact binary inputs and 288 source packages. Its
reproduced runtime failure remains a separate acceptance issue. The supplementary
collector verifies publisher HTTPS
provenance and hashes; it does not verify Debian developer PGP signatures or
reproduce the distributor's original build.

Each Mac graph contains 35 Homebrew packages plus custom mpv, ANGLE and
libplacebo builds. Installed recipes/SBOMs or verified legacy receipts identify
the source archives, resources and patches. Shaderc's own pinned glslang/SPIR-V
resources are included. Custom libplacebo preserves its pinned generated/header
inputs, and the inventory separately identifies the exact external `xxhash.h`.
The selected OpenGL build has no Vulkan-loader runtime dependency. Review of
these installed recipes found no further external static/header input beyond
those captured. The Intel recipes were compared separately: its older libvpx,
x265, MuJS, Little CMS and HarfBuzz releases have their own matching archives;
the recipe differences introduced no additional external code inputs. This
conclusion applies to these graphs, not future upgrades.

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
also retain libdovi's own locked Rust dependencies. The current libdovi 3.4.0
lockfile contains 90 packages. Two old winapi import-library crates omitted
licenses and Cargo revision metadata; their checksum-bound fallback verifies
all 1,390/i686 and 1,419/x64 payload files against release commit
`9497609ef44cc9bcd16cd2411c0ee6ccaf5483aa`, including their prebuilt import
archives. That source tree also retains definitions, Cargo build scripts and
original MIT/Apache-2.0 notices; it does not document a tested regeneration
recipe for the prebuilt imports. Full local capture passes; run 38209801548
passed this capture before failing later on Rust compiler-source fixtures. Custom FFmpeg excludes
Whisper/GGML and preserves its source, patch and explicit configure recipe.
The Vulkan loader also uses generated/header inputs from Vulkan-Headers
1:1.4.363.0-1, newer than libplacebo’s 1:1.4.350.1-1. A local source/recipe
comparison verified that additional archive and three notices; hosted capture
of this new edge passed in run 38221142186. Its complete collection covers 118
package identities, 109 source packages and 5,110 native notice records, before
the separately captured libdovi Rust notices. Thirty-eight independently reviewed
source/recipe hashes match that hosted inventory. The final downloadable archive
and installed-package comparison remain required: NSIS could not open one of
the expanded Rust license paths.

The Rust 1.97.0-1 MSYS2 source archive
(`1af86af91b610412b6d7c4bf958617dc85e15aa6d87c4a76411c581d5ebc41e1`)
contains two Insta multipart-extension snapshots named `.tar.gz` but holding
literal `test` bytes. Their upstream tests confirm that intended content. The
collector records the two exact version/path/content exceptions rather than
attempting decompression; unknown malformed archives still fail and no source
bytes are removed. Local capture and independent hash checks cover all 3,800
notices and both fixture records. Run 38213750998 passed that source capture
before encountering the later Vulkan-header epoch mismatch; the final Windows
package audit remains open.

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
The hosted platform evidence from run 38206642301 independently verifies both
publisher documents, their derived text, 15 attribution records and both
linker-response hashes. C runtime end-user terms will accompany installed
notices when bundling completes.

The WebView2 loader is another explicit boundary: all nine prebuilt files in
`webview2-com-sys` 0.39.1 were matched to Microsoft.Web.WebView2 SDK 1.0.3800.47.
The MSVC GUI uses the x64 `WebView2LoaderStatic.lib`. The archived SDK supplies
BSD-3-Clause license text and notices, **not loader implementation source**.
Its archive is binary provenance, not recovered corresponding source. The
Evergreen WebView2 Runtime is separately installed by the bootstrapper. Keep
this source-availability limit visible during final redistribution review;
do not describe the Windows archive as an entirely source-built native stack.

The Windows installer adds a separate inventory requirement: the pinned Tauri
CLI 2.12.0 selects NSIS 3.11 and `nsis_tauri_utils` 0.5.3. These are embedded
installer code, outside the GUI/CLI dependency graph. The NSIS binary/source
archives and helper release source are recovered, but the helper's release tree
has no Cargo.lock and its publication
command uses an unlocked Cargo build. Its binary hash therefore does not establish
the versions of its embedded Rust dependencies. A newly generated lockfile must
not be presented as evidence of those historical versions.

The workflow now builds that helper from its pinned release source with VSF's
pinned Rust toolchain and an explicit lockfile. It captures its sources, notices,
standard-library inputs and SDK link identities. A generated template changes
one plugin-directory directive; the final EXE must embed the exact built helper
and unchanged plugins from the pinned NSIS distribution. The native source archive
includes NSIS sources, notices, matching Tauri templates and the build recipe.
Local source capture verified four archives and all seven registry dependencies;
unit controls reject missing/changed embedded plugins and a changed NSIS cache.
The i686 helper now has two retained VSF patches: honoring Rust allocation
alignment and retaining the token-user buffer while comparing process ownership.
[Run 38220455020](https://github.com/golgeek/visualsubforge/actions/runs/38220455020)
reproduced the original allocator's alignment failure, passed all seven helper
tests after patching (including 36 allocation layouts), and compiled the release
DLL. It stopped later in platform metadata collection. The Windows environment
key lookup and DLL export-definition parser are corrected. Replacement 38221142186
passes the complete helper inventory, with 45 standard-library files and ten
linker inputs. Its final installer failed on a separate long notice path. Windows
now stages byte-identical notices under short names, with an index preserving
each original path and checksum. The pinned-NSIS path regression and completed
installer verification remain pending.
See [the recipe and evidence scope](../tests/native/nsis-installer/README.md).
Sources: [pinned Tauri bundler](https://github.com/tauri-apps/tauri/blob/447fa9f3f993fe77724189e355078b38ce20baea/crates/tauri-bundler/src/bundle/windows/nsis/mod.rs),
[helper release source](https://github.com/tauri-apps/nsis-tauri-utils/tree/13d9edd27b69310e108d6fbd49f90992f8a05390)
and [publication recipe](https://github.com/tauri-apps/nsis-tauri-utils/blob/13d9edd27b69310e108d6fbd49f90992f8a05390/.changes/config.json).

## Acceptance still required

Finish the pending table entries, review their actual versions and compare the
installed inventories/notices with the matching downloadable source archives.
Windows installation must pass the private DLL load/unload controls and packaged
GUI smoke test; Linux's audited package passes the isolated CLI and ready-device
virtual playback controls, with cold physical-device startup still untested.
The Mac signing/source audits above have passed. Update this
review to the final Windows/Linux candidates when their checks complete.

The Ubuntu installed GUI evidence uses Xvfb, Mesa llvmpipe and a PulseAudio null
sink. It establishes neither visible pixel correctness nor audible output.
All platforms still need the [manual acceptance checklist](preview-acceptance.md),
including clean consumer systems, real display/audio devices and claimed oldest
OS versions. No release has been published.
