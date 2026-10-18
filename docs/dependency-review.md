# Preview dependency review

Runtime correction, 2026-10-12: the original Mac candidates from 38205417832
are withdrawn after reproducing a hardened-runtime termination in mpv's unused
LuaJIT `stats` script. The audited replacements below disable those scripts;
both pass hardened initialization and Apple notarization. The exact ARM download
also passes native smoke and older-project restoration on macOS 27. See
[manual acceptance](preview-acceptance.md) for the remaining device/install gates.

This review concerns the actual candidate artifacts below. A completed collector
means that its declared inputs were captured; it is not, by itself, a review of
all code embedded in a binary. Release publication and manual runtime acceptance
remain separate gates in [releases.md](releases.md).

## Artifact evidence

| Target | Candidate evidence | Independent verification | Remaining work |
| --- | --- | --- | --- |
| Ubuntu 24.04 x64 | [38208853449](https://github.com/golgeek/visualsubforge/actions/runs/38208853449), commit `3c13449` | Three download hashes; 124 native files; 120 observed private GUI libraries; 1,063 application-source files; 1,850 native build-input evidence files and 3,271 installed notices | Installation/offline CLI passed. Cold null-sink diagnostics failed in 2/10 trials; [38212585168](https://github.com/golgeek/visualsubforge/actions/runs/38212585168) independently measured sink startup delay and passed 10/10 with sink readiness checked before VSF. Physical-device acceptance remains open. |
| macOS ARM64 | [38331848710](https://github.com/golgeek/visualsubforge/actions/runs/38331848710), commit `339e51e`, `macos-unsigned-aarch64` | Four download hashes; 49 native files; all 1,609 inventoried app files; 1,325 native source-evidence files and 959 installed native notices | Signed/notarized artifact verified separately; manual device/oldest-OS acceptance remains open. |
| macOS Intel | Same package matrix run; commit `339e51e` | Four download hashes; 49 native files; all 1,609 inventoried app files; 1,324 native source-evidence files and 959 installed notices; signed/notarized archive independently verified | Manual device/oldest-OS acceptance remains open. |
| Windows x64 | [38227273736](https://github.com/golgeek/visualsubforge/actions/runs/38227273736), commit `3410bd4` | Five artifact hashes; 44 native files; all 962 packaged files; 6,434 native source-evidence files and 912 application-source files; 6,044 logical notice records map to 911 byte-identical texts | Installation, offline resync/input preservation and DLL unload pass. [Replacement 38233864612](https://github.com/golgeek/visualsubforge/actions/runs/38233864612) passes the unchanged GUI smoke with virtual WASAPI and software D3D11. Physical/consumer-OS acceptance remains open. |

The Ubuntu `.deb` SHA-256 is
`4ed408d688ff09fa0a2f14db6927b58aea36a8addc85bef8d3c785f539865849`.
Its embedded Ubuntu build-input inventory SHA-256 is
`7bbd4cbd940effc8e355ca4c547101d008c03c638d6e3c8be88a74752bd7b166`.
The ARM Mac native inventory SHA-256 is
`2751a023c6319bf6eadda7ec4f3c96849f7d6977b24cf73d4b327d518bad7fe0`.
Signing changes Mac native file hashes; use the later signing receipt to verify
the distributed signed app, not the unsigned file hashes above.

The replacement signed Mac downloads reuse those exact unsigned packages:

| Architecture | Signed app artifact | Final ZIP SHA-256 |
| --- | --- | --- |
| Apple Silicon, macOS 14+ build floor | [macos-notarized-aarch64](https://github.com/golgeek/visualsubforge/actions/runs/38334674579/artifacts/11784371040) | `42a236cb4b00f533e694a4ff0b4a7ccc07eb9273e066db7aef4f551058f610a7` |
| Intel, macOS 15+ build floor | [macos-notarized-x86_64](https://github.com/golgeek/visualsubforge/actions/runs/38335057462/artifacts/11784079472) | `0b4a28600e42b785e2d81ec4a9e1db41fac70d716e28c5865525fbf14d3c2f01` |

The corresponding `macos-unsigned-*` artifacts contain the matching native and
application source archives ([ARM](https://github.com/golgeek/visualsubforge/actions/runs/38331848710/artifacts/11782814984), [Intel](https://github.com/golgeek/visualsubforge/actions/runs/38331848710/artifacts/11783364443)).
All 37 captured recipe texts and both native build-option sets match the
previously reviewed baseline. Six source archives changed compressed hashes;
independent comparison found identical file/link payloads. Homebrew receipt and
SBOM differences are capture timestamps. Application archives bind the source
commit above and all 889 source/notice files per architecture.
Independent download checks verified each final
ZIP, all 49 signed native hashes, and 1,558 unchanged inventoried resources per
architecture against its unsigned assembly. Apple accepted both submissions;
local `codesign --verify --deep --strict`, `xcrun stapler validate` and
`spctl --assess --type execute` passed for both extracted apps. These are
signature/distribution checks on the development Mac, not playback or oldest-OS
tests. Each signing receipt also binds the successful hardened initialization
probe to its unsigned input and the signed packaged libmpv hash. ARM native
smoke and project restoration are recorded separately in [releases.md](releases.md).
The Intel native inventory SHA-256 is
`2692aa49a41081966cf5177c17dc49e639e6701a272b12de2073a681f3d6a69d`.

The Windows [candidate artifact](https://github.com/golgeek/visualsubforge/actions/runs/38227273736/artifacts/11738654119)
contains the unsigned NSIS installer, portable ZIP, both source archives and
checksums. Its installer SHA-256 is
`657720d72e896c70d3b0308be3f4437f03a949eb31625f276a5778a7f1f3d4fb`;
the portable ZIP is `5e653e7d301af5aed23c6f02a61057006c9fc455e6621fbf11976ae4e696db11`.
Native/application source archive hashes are respectively
`09790e19ab3a939426fdac50e9708cf9c19efed41bdff680a0697424f578e61a` and
`55edc0f016bdf25119721b3e3e4be56202a72b8742df67581d9f034e68d693a9`.
Independent audits bind every archived notice to its installed compact-index
entry, all source manifest files to their hashes and all 40 reviewed recipes to
the actual archive. Windows application capture includes 289 Cargo packages,
282 checksum-verified registry archives, four frontend packages and 45 Rust
standard-library files. The installed 962 files match assembly byte-for-byte;
The unchanged GUI smoke subsequently passes in 38233864612 with virtual WASAPI
and software D3D11; physical-device acceptance remains separate.

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
the separately captured libdovi Rust notices. Forty independently reviewed
source/recipe hashes, including both historical SPIR-V versions, now match the
downloaded 38227273736 archive. Compact notice staging resolves the NSIS path
limit; the actual installer preserves every original notice byte and owner.

The Rust 1.97.0-1 MSYS2 source archive
(`1af86af91b610412b6d7c4bf958617dc85e15aa6d87c4a76411c581d5ebc41e1`)
contains two Insta multipart-extension snapshots named `.tar.gz` but holding
literal `test` bytes. Their upstream tests confirm that intended content. The
collector records the two exact version/path/content exceptions rather than
attempting decompression; unknown malformed archives still fail and no source
bytes are removed. Local capture and independent hash checks cover all 3,800
notices and both fixture records. Run 38213750998 passed that source capture
before encountering the later Vulkan-header epoch mismatch; the final Windows
source/notice package audit passes in 38227273736.

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
linker-response hashes. C runtime end-user terms accompany the installed
notices; their source and compact-index hashes match in 38227273736.

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
installer verification pass in 38227273736. The independently audited receipt
compares 441 NSIS cache files and all five embedded plugins, including the exact
source-built helper, with their pinned or built inputs.
See [the recipe and evidence scope](../tests/native/nsis-installer/README.md).
Sources: [pinned Tauri bundler](https://github.com/tauri-apps/tauri/blob/447fa9f3f993fe77724189e355078b38ce20baea/crates/tauri-bundler/src/bundle/windows/nsis/mod.rs),
[helper release source](https://github.com/tauri-apps/nsis-tauri-utils/tree/13d9edd27b69310e108d6fbd49f90992f8a05390)
and [publication recipe](https://github.com/tauri-apps/nsis-tauri-utils/blob/13d9edd27b69310e108d6fbd49f90992f8a05390/.changes/config.json).

## Acceptance still required

The candidate inventories and source/notice comparisons above are complete for
their documented scope. Windows installation, private DLL load/unload, offline
CLI and GUI smoke pass; the GUI uses software D3D11 and a virtual WASAPI endpoint.
Linux passes isolated CLI and prepared virtual-device playback; the Mac
signing/source audits pass for both architectures. These candidates use the same
application implementation; the Mac revision predates only additional smoke-test
observation logging. Their native libraries and package recipes remain separately
identified by each archive. None is a claim of a reproducible rebuild of every
vendor binary or recovery of the proprietary platform sources described above.

The Ubuntu installed GUI evidence uses Xvfb, Mesa llvmpipe and a PulseAudio null
sink. It establishes neither visible pixel correctness nor audible output.
All platforms still need the [manual acceptance checklist](preview-acceptance.md),
including clean consumer systems, real display/audio devices and claimed oldest
OS versions. No release has been published.

## October18 Linux feature validation candidate

[Linux package39142923333](https://github.com/golgeek/visualsubforge/actions/runs/39142923333)
builds f0dbf68c643d4f9a483389fe07b4103453c26c29. The
[candidate and both source archives](https://github.com/golgeek/visualsubforge/actions/runs/39142923333/artifacts/12143295022)
and [installation/native evidence](https://github.com/golgeek/visualsubforge/actions/runs/39142923333/artifacts/12142293247)
are retained. Downloaded hashes verify:

| File | SHA256 |
| --- | --- |
| `VisualSubForge_0.0.1-alpha_amd64_ubuntu24.04.deb` (80,124,236bytes) | `f251f421c04d25bb47e2a2936e46257157d45b81458b8a358aea92f26a5f9379` |
| `native-sources.tar.gz` | `647c718bb5614755239acafa8cc02c652a66262e03251cb21cd27454d139630b` |
| `application-sources.tar.gz` | `a538fac817231b1493b23ffc5cc408ec000b32391ae45ef15cbd9c15d842d91e` |

The independent artifact identity check verifies1092 application source/notice
files and713 installed notices. The inventory contains369 resolved Cargo packages;
registry crate hashes match the archived lockfile. Four frontend packages remain inventoried. The new rquickjs
0.14.0 family carries MIT notices, with the missing rquickjs-core notice supplied
from its recorded upstream revision; rquickjs-sys retains its QuickJS notice.
Spellbook0.4.2 carries MPL-2.0 notices/source. All nine VSF dictionary data/source/
notice files are present verbatim in the application source archive. This is
source and notice identity evidence, not a new blanket redistribution approval.

All124 assembled runtime files and120 observed private images match package
bytes, including GUI b35e3d49... and libmpv6e5c0f0d.... The custom mpv/libplacebo
sources and Ubuntu build-input inventory match the receipt. Exact audit script
and results live under `test-results/native-reliability-20261018/linux-package-f0db-*`.
Isolated installation, offline CLI and software-renderer/null-sink smoke pass;
physical GPU/audio, interactive dialogs and oldest-OS acceptance remain separate.
