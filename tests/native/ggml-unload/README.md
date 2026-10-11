# Optional GGML exception-handler experiment

Experimental dependency fix for [task 005](../../../docs/tasks/005-audio-loop-timing.md) and the Windows lifetime gate in [003d](../../../docs/tasks/003d-packaging-decision.md). This is separate from the mpv audio-clock patch series and is not installed in the application or guest toolchain.

GGML 0.25.3 installs a process-wide C++ terminate handler during library initialization, but does not restore it when its DLL unloads. A shared C++ runtime can retain a pointer into unmapped code. Restoring an earlier handler in a destructor would also require reasoning about other libraries that chain handlers and their unload order. This experiment avoids taking ownership of the host's handler at all.

The patch adds `GGML_EXCEPTION_BACKTRACE`, default **ON** for compatibility with the original build. **OFF** omits only the C++ terminate-handler registration and its callback. The host's existing terminate policy remains in force; GGML's explicit assertion/backtrace implementation is unchanged. It requires no process environment setting and adds no unload-time synchronization or callbacks. The tradeoff is losing GGML's automatic backtrace for uncaught C++ exceptions.

[manifest.json](manifest.json) pins the official v0.25.3 source archive and patch hashes. Source context is from GGML under the accompanying [MIT notice](LICENSE.MIT); no legacy VSS source is involved. The archive hash also matches the reviewed MSYS2 0.25.3 package recipe. Verify both hashes before extracting/applying into a fresh external directory.

In the Windows CLANGARM64 development shell, after verifying/extracting the archive and applying the patch with `git apply --check` followed by `git apply`:

```sh
cmake -S ggml-0.25.3 -B build-off -GNinja \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON \
  -DCMAKE_DLL_NAME_WITH_SOVERSION=ON -DGGML_NATIVE=OFF -DGGML_LTO=ON \
  -DGGML_CPU=OFF -DGGML_OPENMP=ON -DGGML_CCACHE=OFF \
  -DGGML_BUILD_TESTS=OFF -DGGML_BUILD_EXAMPLES=OFF \
  -DGGML_EXCEPTION_BACKTRACE=OFF
cmake --build build-off --target ggml-base -j 6
clang++ -std=c++17 -Wall -Wextra -Werror -municode \
  windows-dll-reload-probe.cpp -o reload.exe
./reload.exe C:/absolute/path/build-off/bin/ggml-base-0.dll reload unchanged
```

Build a separate `build-on` with the option ON for the failure control. These commands build only `ggml-base`; disabling CPU-backend construction here does not replace or disable the installed CPU backend. An isolated runtime can put this base DLL beside a copied probe or player executable while resolving remaining libraries from the development toolchain. Observe the actual loaded DLL path and unloading, not just the process exit code. This is a diagnostic deployment, not a clean-machine package or a general ABI guarantee.

The optional `unchanged` argument in [the reload probe](../windows-dll-reload-probe.cpp) checks handler identity while loaded and after unloading, as well as whether the handler's owning module exists. It reports the selected GGML module path without changing module reference counts and never calls a dangling handler.

On the supplied Windows 11 ARM64 Parallels VM, both builds compile successfully. The ON direct reload control aborts with `0xc0000409`; OFF completes three unload/reload cycles with its original handler unchanged. The same OFF base DLL also passes three unload/reload cycles through stock and experimental libmpv, two upstream core-lifetime runs and six null/WASAPI property transport checks. ON controls retain dangling handlers through both libmpv builds. See [task 005](../../../docs/tasks/005-audio-loop-timing.md#windows-ggml-unload-remedy-candidate--2026-10-03) for exact binary provenance, retained harness failure, evidence and remaining release gates. No physical Windows hardware, MSVC runtime, or x86-64 validation is implied.
