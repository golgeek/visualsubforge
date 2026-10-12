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
not redistributed. A complete installer receipt still requires a hosted build.

On a Windows runner with Python 3.14.7, the pinned pefile requirement and an x86
MSVC developer environment:

```powershell
python scripts/windows-installer.py build --build "$env:RUNNER_TEMP/installer"
```

The script runs the upstream workspace tests and builds the i686 helper outside
the checkout. `installer-build.json` records source hashes, Rust/SDK versions,
actual linker inputs and the helper's system imports. Its commands log separates
test execution from compilation; zero-test harnesses are not feature coverage.

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
