# Windows in-process libmpv surface probe

[windows-libmpv-surface.cpp](windows-libmpv-surface.cpp) tests D3D11 output into a child HWND in the host process. It complements the separate-player WebView2 composition probe; it contains no WebView, waveform or accepted subtitle document. See [task 003c](../../docs/tasks/003c-desktop-composition.md#windows-in-process-hwnd-boundary--2026-10-03) for results and retained failures.

## Build and run

In the existing Windows MSYS2 CLANGARM64 development shell with mpv headers:

```sh
clang++ -std=c++17 -Wall -Wextra -Werror -municode windows-libmpv-surface.cpp -o probe.exe -luser32
./probe.exe C:/absolute/probe/libmpv-2.dll C:/absolute/dependency/bin C:/absolute/composition.mp4 C:/absolute/preview.srt --run
./probe.exe C:/absolute/probe/libmpv-2.dll C:/absolute/dependency/bin C:/absolute/composition.mp4 C:/absolute/preview.srt --run --close-smoke
./probe.exe C:/absolute/probe/libmpv-2.dll C:/absolute/dependency/bin C:/absolute/composition.mp4 C:/absolute/preview.srt --run --interactive
```

The fixture is six seconds, 640×360 at 10 fps. Its SRT contains `Initial café — 日本語` and `Second line` from 0.5 to 2 seconds, then `Second cue` from 3 to 4.5 seconds. The sequence checks cue/gap state, paused replacement, resize bounds, advancing playback, pause, stop and reload. Replacement uses a synthetic `memory://` SRT; this is not a document revision protocol. The assertions inspect properties/events and window geometry, not presented pixels. A missing media path must fail with exit 1 and complete native cleanup; invalid arguments return 2.

Ordinary execution exits after the sequence. `--interactive` then plays repeatedly for at most 120 seconds or until Close. `--close-smoke` posts `WM_CLOSE` after the sequence and requires the main-thread handler to acknowledge it within five seconds. This exercises the same handler but does not verify mouse/keyboard delivery. Logs distinguish window-close from deadline. Use a supervising process with a longer timeout to detect a native teardown hang; the worker's stage deadlines cannot bound a blocking library call.

## Ownership

The main thread creates and sizes the parent/child HWNDs. One worker loads the DLL and owns every libmpv client call, event pointer, returned string and core. mpv's `vo=gpu` owns its D3D11 output machinery; this probe does not use the render API or transport decoded frames.

The worker requests main-thread resizing by a posted message and waits for an atomic acknowledgement. Load/reload waits for a new `FILE_LOADED` event; old video properties alone are insufficient. Stop similarly waits for its end-file event and idle state.

Close sets an atomic stop request and returns immediately. The worker destroys the core, unloads its DLL reference, checks that the original C++ terminate handler remains unchanged, and posts completion. Only then does main destroy the HWNDs and join the completed worker. This demonstrates ordinary, missing-media and requested-close paths, not every possible Win32 message-loop failure or a hard cancellation guarantee.

DLL lookup uses an absolute library path, its directory and a process-local dependency search directory. The recorded run uses private copies of the earlier **eight-patch** mpv candidate and the [GGML exception-handler OFF experiment](ggml-unload/README.md). Remaining DLLs resolve from the installed CLANGARM64 toolchain. The installed libraries are unchanged. This is not the current nine-patch audio candidate, an EOF-remedy test, a clean-machine package or a production dependency decision.

## Scope

Measured on Windows 11 ARM64 in Parallels: D3D11 feature level 11_1 through the Parallels WDDM adapter, with software decoding despite `auto-copy`. The native ARM64 executable loads libmpv in-process. Visible playback and Unicode subtitles were observed; mouse resize/maximize/close and Alt-F4 attempts in the follow-up did not produce observable actions. That run ended at its timed bound. Keep those input checks unverified.

Rust/MSVC linkage, actual WebView2/Tauri coexistence, clipping/focus/modal routing, blank pixels after unload, live DPI changes, rapid replacement/close races, physical GPU drivers, hardware decoding and x86-64 are still separate requirements. A geometry or subtitle-property pass does not establish presentation timing or task 005 audio correctness.
