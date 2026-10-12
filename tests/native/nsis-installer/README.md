# Windows installer inputs

The installer embeds code beyond VSF's GUI/CLI. Tauri CLI 2.12.0 selects NSIS
3.11 and `nsis_tauri_utils` 0.5.3. The helper's published source has no Cargo.lock
or retained compiler pin, so its published DLL cannot supply an exact Rust
dependency inventory. VSF builds that release source with its own pinned Rust
toolchain and the lockfile here. This lock identifies our build, not the
historical upstream binary.

`sources.json` pins the NSIS binary/source archives, helper source and matching
Tauri source. NSIS retains its original licenses, including the LZMA linking
exception in COPYING; the helper and Tauri retain MIT/Apache-2.0 notices. The
collector includes all locked Rust source archives and the checksum-verified
i686 standard-library component and its sources. SDK link inputs are hashed,
not redistributed. The helper build/inventory passed in run 38221142186; the
final installer receipt and independent archive comparison pass in run 38227273736.

On a Windows runner with Python 3.14.7, the pinned pefile requirement and an x86
MSVC developer environment:

```powershell
python scripts/windows-installer.py build --build "$env:RUNNER_TEMP/installer"
```

The script runs the upstream workspace tests and builds the i686 helper outside
the checkout. `installer-build.json` records source hashes, Rust/SDK versions,
actual linker inputs and the helper's system imports. Its commands log separates
test execution from compilation; zero-test harnesses are not feature coverage.

The allocator patch handles Rust alignments greater than Win32 HeapAlloc's
eight-byte guarantee. Its control must fail against the original allocator,
then pass alignment, zeroing, growth, shrinkage and preservation checks against
the patched implementation. Original source, patch and control are all retained.
VSF modifications/controls use the repository's GPL-3.0-or-later license; original
upstream notices remain intact. [Run 38220455020](https://github.com/golgeek/visualsubforge/actions/runs/38220455020)
reproduced the original alignment failure, then passed the 36-layout control,
five process tests (including SID ownership) and one version-comparison test.
It compiled the release DLL with only kernel32/user32/advapi32/shell32 imports.
The job subsequently failed collecting platform metadata. Replacement
[38221142186](https://github.com/golgeek/visualsubforge/actions/runs/38221142186)
passes the complete helper build/inventory, including 45 standard-library files
and ten actual linker inputs. Actual installer verification passes in run 38227273736, checking all 441 NSIS
cache files and the five embedded plugins against their selected inputs.
The tests run serially because the plugin API stores a global NSIS stack and
the process-launch tests temporarily change the same process token's privileges.
The earlier parallel run's failure remains evidence, not a passed check.
See [HeapAlloc alignment](https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc)
and [Rust's allocator contract](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html).

The SID ownership patch keeps the `GetTokenInformation` allocation alive while
process ownership is compared. Upstream returned a raw pointer into a local Vec
that had already been dropped. The new owner holds an explicitly aligned buffer;
its test compares two live queries, the current process and an invalid process.
This is a separate source-confirmed lifetime defect, not a claimed explanation
for the earlier test crash.

The ordinary package workflow restores the x64 compiler environment for VSF. It
attaches installer sources and notices before bundling, then sets
`VSF_NSIS_PLUGIN_DIR` to the private helper directory. The custom template is
generated from the pinned original by replacing exactly one `!addplugindir`
directive. No copied template fork or change to Tauri's cache verification is
required. When upgrading Tauri, review that substitution and the new inputs.

After bundling, the verifier compares the actual NSIS cache with the pinned
distribution and extracts every installer plugin with 7-Zip. The embedded helper
must match the source build; other embedded plugins must match NSIS 3.11.
`installer-receipt.json` binds those hashes to the final installer. Successful
actual installation, resource comparisons and runtime checks remain separate
requirements; native display/audio acceptance is manual.

The first full native-source capture exposed a separate NSIS limitation: an
expanded Rust license path exceeded its file API's path limit. Windows assembly
now copies notice bytes into short `third-party/files/<sha256>.txt` names.
`third-party/index.json` retains every original notice path and checksum;
identical texts can share a file without losing their owners. Source archives
retain their original paths, and `native-sources/installed-notices.json` plus
the assembly receipt bind the installed index. This changes only notice layout.

Before compiling the helper or media libraries, the build now invokes the pinned
NSIS compiler on a deliberately long source path, requires the known failure,
then compiles and silently installs the compact layout and checks every byte.
This real Windows control and the completed application installer both pass
in run 38227273736; actual installation verifies all 962 assembled files. The portable unit control covers distinct notices with the
same basename, shared identical notices, long paths and non-UTF-8/CRLF bytes.
