// Isolated Windows dependency diagnostic. Intentionally import libc++ so its
// process-wide terminate handler survives unloading the requested DLL. This is
// not the application's media-core lifecycle and initializes no mpv instance.
// Usage: windows-dll-reload-probe.exe ABSOLUTE_DLL_PATH reload|keep [unchanged]
// Exit 3 reports a handler without a loaded owning module; a dependency may
// itself abort first. Never invoke the potentially dangling handler.
#include <windows.h>
#include <cstdio>
#include <cwchar>
#include <exception>

static bool report_handler(const char *stage, int iteration,
                           std::terminate_handler expected = nullptr) {
    void *handler = reinterpret_cast<void *>(std::get_terminate());
    HMODULE owner = nullptr;
    // FROM_ADDRESS interprets this argument as an address, not a string.
    // UNCHANGED_REFCOUNT observes ownership without pinning the module.
    bool found = GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
            GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        reinterpret_cast<LPCWSTR>(handler), &owner);
    DWORD error = found ? ERROR_SUCCESS : GetLastError();
    std::printf("%s %d terminate=%p owner=%p lookup_error=%lu\n",
                stage, iteration, handler, owner, error);
    bool unchanged = !expected || std::get_terminate() == expected;
    if (expected) std::printf("handler_unchanged=%d\n", unchanged);
    // Observe the actual transitive dependency selected by the Windows loader.
    // Neither lookup changes its reference count or prevents a later unload.
    HMODULE ggml = GetModuleHandleW(L"ggml-base-0.dll");
    wchar_t path[32768] = {};
    if (ggml && !GetModuleFileNameW(ggml, path, 32768)) return false;
    std::printf("ggml_loaded=%d path=%ls\n", ggml != nullptr, path);
    std::fflush(stdout);
    return found && unchanged;
}

int wmain(int argc, wchar_t **argv) {
    if ((argc != 3 && argc != 4) || (std::wcscmp(argv[2], L"keep") &&
                      std::wcscmp(argv[2], L"reload")) ||
        (argc == 4 && std::wcscmp(argv[3], L"unchanged"))) return 2;
    bool keep = std::wcscmp(argv[2], L"keep") == 0;
    const auto expected = argc == 4 ? std::get_terminate() : nullptr;
    bool stale = !report_handler("initial", -1, expected);
    HMODULE library = nullptr;
    for (int i = 0; i < 3; ++i) {
        stale |= !report_handler("before-load", i, expected);
        if (!keep || !library) library = LoadLibraryW(argv[1]);
        if (!library) {
            std::printf("load failed %lu\n", GetLastError());
            return 1;
        }
        stale |= !report_handler("loaded", i, expected);
        if (!keep) {
            if (!FreeLibrary(library)) return 1;
            library = nullptr;
            stale |= !report_handler("unloaded", i, expected);
        }
    }
    // The keep control leaves the library loaded until process teardown. It
    // must not introduce the very runtime unload that it is comparing against.
    return stale ? 3 : 0;
}
