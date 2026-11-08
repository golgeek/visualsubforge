# Optional GGML exception-handler experiment

Experimental dependency fix for a Windows DLL-unload lifetime failure. This is separate from the mpv audio-clock patch series and is not installed in the application or guest toolchain. See the [native maintenance contract](../../../docs/native-runtime.md#maintenance-ownership) for dependency ownership and adoption requirements.

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

On the supplied Windows 11 ARM64 Parallels VM on 2026-10-03, both builds compile successfully. The ON direct reload control aborts with `0xc0000409`; OFF completes three unload/reload cycles with its original handler unchanged. The same OFF base DLL also passes three unload/reload cycles through stock and experimental libmpv, two upstream core-lifetime runs and six null/WASAPI property transport checks. ON controls retain dangling handlers through both libmpv builds.

The OFF DLL SHA-256 is `ef4f2a4cea5b9453eab7458236d4392328b1c94883ec0b73089c8ca611f09ee0`; ON is `ec95e0d04b99498c7fb26968d796802bcd790972f2b3850e441ea6ba4526b3b4`. These runs used Clang 22.1.8/CMake 4.4.3-3 and the earlier eight-patch libmpv candidate, not the current audio recipe. The first integration runner stopped before transport playback because a helper was missing; staging the unchanged helper allowed the six transport checks to complete. That setup failure is not a passing run.

Production adoption still requires selecting and maintaining the dependency graph and validating the actual MSVC/Tauri host and library lifetime. GGML computational backends, clean-machine packaging, physical Windows hardware and x86-64 were not validated. Property checks do not establish captured audio or acoustic timing; the [native adoption gates](../../../docs/native-runtime.md#remaining-adoption-gates) remain separate.
