# Synthetic media fixtures — task 003a

The [resynchronization corpus](resync/README.md) adds independently generated edit
maps for matching programme blocks across ad edits, trims and frame-rate changes.

Use [current status](../../docs/status.md) for today's implementation and validation
limits. Task-numbered sections below retain dated experiments; links into local
untracked research may be unavailable in a fresh checkout. Fixture generation
and the tracked scripts do not require those notes.

All signals and images are generated from FFmpeg lavfi expressions in `scripts/media-fixtures.py`. No source material, fonts, or legacy assets are imported. Generated files are ignored by Git. FFV1/PCM in Matroska avoids encoder delay in the offset experiment; this does not demonstrate compressed-codec delay handling.

`subtitles/fixture-font.ttf` and `embedded-font.ass` are original synthetic font
fixtures under the repository's GPL-3.0-or-later license. Their two visible glyphs
are a triangle and a rectangle, authored in `scripts/font-fixture.py`; no existing
font outlines or metadata were copied. Regenerate with fontTools4.61.1 in an
isolated Python environment. Fixed font timestamps make output reproducible.
The checked-in files require no fontTools build/runtime dependency and are never
installed as system fonts. See [preview font validation](../../docs/preview-fonts.md).

## Generate and validate

Install FFmpeg/ffprobe, then run from the repository root:

```sh
python3 scripts/media-fixtures.py
cargo test --locked -p media-analysis
cargo test --locked -p media-analysis --test ffmpeg -- --ignored --nocapture --test-threads=1
cargo run --locked -p media-analysis -- tests/fixtures/generated/vfr-offset.mkv 0 > /tmp/vsf-waveform.json
```

For this Mac, use the native tools explicitly (the older `/usr/local/bin` installation is Intel):

```sh
python3 scripts/media-fixtures.py --ffmpeg /opt/homebrew/bin/ffmpeg --ffprobe /opt/homebrew/bin/ffprobe
VSF_FFMPEG=/opt/homebrew/bin/ffmpeg VSF_FFPROBE=/opt/homebrew/bin/ffprobe cargo test --locked -p media-analysis --test ffmpeg -- --ignored --nocapture --test-threads=1
```

The generator records versions and complete argv arrays in `generated/generation.json`, and overwrites only its named generated outputs. It verifies frame timestamps and stream offsets using ffprobe. Equivalent timestamps/signals, not byte-identical encoder output across FFmpeg versions, are the reproducibility contract. CI installs Ubuntu's FFmpeg package and logs the version; its dependencies are not packaged application dependencies.

| Fixture | Expected project timestamps |
| --- | --- |
| `cfr-offset.mkv` | 100 video frames: 0, 0.04, …, 3.96 s. Audio ordinal 0 starts at 0.25 s, with a positive pulse at 1.25 s. Audio ordinal 1 starts at 0 and has a negative pulse at 2 s. Both end at 4 s. |
| `vfr-offset.mkv` | 70 video frames: 0, 0.04, …, 1.96, then 2, 2.1, …, 3.9 s. Same audio tracks as CFR fixture. No constant frame duration may be inferred for stepping. |
| `long.flac` | 7200 s, 8000 samples/s; positive pulse at 7199 s. 57,600,000 samples and 720,000 ten-millisecond buckets. |

The CLI emits JSON containing stream index, container origin in seconds, sample rate, samples per peak, sample count and min/max peaks. Peak `i` starts at project time `i * samples_per_peak / sample_rate`. The last bucket can be partial: use sample count for its actual end.

## Large daily-use editing fixture — task006

```sh
python3 scripts/daily-use-fixture.py --output tests/fixtures/generated/006-daily-use-fixture-v1 --ffmpeg /opt/homebrew/bin/ffmpeg --ffprobe /opt/homebrew/bin/ffprobe
```

Choose a fresh output directory; the generator refuses existing directories and
FFmpeg refuses overwrites. `workflow.mkv` contains two hours of 1920×1080 H.264
at 30 fps and stereo 48 kHz FLAC. It repeats a synthetic 12-second test pattern
and continuous tone 600 times, using stream copying after the seed encode.
`workflow.srt` contains 10,000 two-line UTF-8 cues with café and 日本語, beginning
at 1 s, spaced 700 ms apart and lasting 550 ms. The last cue ends at 7,000.850 s,
inside the media duration. Copy the SRT to a disposable session directory before
editing, preserving the original manifest for byte comparisons.

The generator records exact argv, durations, tool versions, media metadata and
file hashes. The Mac v1 generation passed with FFmpeg/ffprobe 9.0.2; a separate
decoded-frame probe near 7,198 s confirms accessible 1080p tail frames. This is
a dense synthetic editing/memory workload. Repeated imagery and tone do not
represent real-film decoder load, dialogue usability or precise audio continuity
at container joins. Use task005's separate content/timing fixtures for audio
acceptance. Task006's remaining-work tracker specifies the unchanged session
budgets; generating a fixture does not satisfy them.

## Limits

This experiment produces one resolution, no cache and no displayed waveform. It downmixes to mono (opposite-phase channels can cancel), and resamples to 8 kHz; peaks are for timing inspection, not mastering measurements. Container origins must be known and finite. Negative/pre-origin audio is trimmed; positive offsets and timestamp gaps are filled with silence by the resampler. Tests establish the supplied positive-offset fixtures only; negative starts, discontinuities, compressed encoder delay, multichannel policy and container-specific timestamp normalization still need fixtures.

Input is a canonical local file, but nested references inside a container are not sandboxed. Do not treat the experimental CLI as an untrusted-media security boundary. The library accepts an overall timeout; the CLI uses 120 s. Output is capped at four hours, probe JSON at 64 KiB and retained diagnostics at 16 KiB. Raw PCM is consumed through a buffered pipe rather than retained in memory. Tests use a blocked helper process to exercise kill/wait cleanup and a large stderr stream to exercise pipe draining.

## Compressed playback fixtures — task 003b

```sh
python3 scripts/playback-fixtures.py --ffmpeg /opt/homebrew/bin/ffmpeg --ffprobe /opt/homebrew/bin/ffprobe
VSF_PLAYBACK_BENCH="$PWD/tests/fixtures/generated" npm run tauri -- dev --features media-prototype -- --locked
```

The separate generator creates `h264-1080p.mp4` (1920×1080 H.264) and `hevc-4k.mp4` (3840×2160 HEVC), each 12 seconds/360 frames at 30 fps, 8-bit SDR, with AAC 440 Hz audio. It inverts the test pattern at exactly 6 seconds, supplying a known visual cut for seek/step checks without implementing scene detection. `playback-generation.json` records tool versions and exact command arguments. Generation is bounded to 300 seconds per subprocess and verifies frame counts/duration; no external media assets are used. Defaults use PATH; the explicit paths above select native Homebrew tools on this Mac.

The opt-in desktop benchmark opens each fixture in software and `auto-copy` hardware-decoding modes, verifies an actual hardware decoder is reported for the latter, performs seven exact seeks, steps forward/backward across the known cut, and samples 10 seconds of playback per case. It writes `generated/playback-results.txt` only on successful completion and leaves the last fixture paused. Use the current terminal PASS result to identify a successful run; an old report can survive a failed rerun. UI interaction during measurement can invalidate results. `VSF_PLAYBACK_BENCH` takes precedence over `VSF_MEDIA_SMOKE` if both are set.

Seek timings measure command-to-`seeking=false` and target position, sampled every 20 ms. They are not physical display latency. Drop counters are libmpv's decoder/output properties and can be unavailable. This short synthetic check is not sustained playback, HDR, real-world media, audible A/V synchronization, CPU/power profiling, or zero-copy validation. Hardware mode copies decoded frames back to system RAM; GPU rendering still remains native.

## Repeated playback and lifecycle stress — task 003b

```sh
VSF_MEDIA_SOAK="$PWD/tests/fixtures/generated" npm run tauri -- dev --features media-prototype -- --locked
```

Use the generated compressed fixtures above and keep the desktop visible. This opt-in run loops HEVC 4K between 1 and 11 seconds for 60 seconds per decoder mode, requiring progress, at least four wraps and a stable reported decoder. It records swap/drop counts and status-request latency. It then measures 30 H.264→HEVC replacements through loaded state plus first swap, 30 closes, and 30 close requests racing an open after generation reservation. Each cancelled-open case joins its test thread and verifies the closed state cannot be resurrected. The cancellation timing begins after reservation; it does not prove native initialization started, nor bound uninterruptible native calls.

Results and failure state are written to `generated/soak-results.txt`. An `Ok(())` result is the pass marker. Latency p95 uses the nearest-rank empirical percentile, and `median_ms` selects the upper middle sample for even counts; these are local debug observations, not accepted latency targets or physical display timings. The longer run still repeats a short synthetic clip; it does not validate long-file demuxing, thermal steady state, power consumption, HDR or audio synchronization. Do not interact with the app during measurement. `VSF_MEDIA_SOAK` takes precedence over the other startup checks. The script leaves HEVC paused on success.

For rendering/lifecycle checks where hardware decoding is unavailable, set
`VSF_MEDIA_SOAK_SOFTWARE_ONLY=1` alongside `VSF_MEDIA_SOAK`. This runs the
60-second software phase and all replacement/close/cancellation checks with
software decoding requested. The report explicitly marks hardware coverage
skipped; the default continues to require both software and hardware modes.
`rendered_frames` measures swaps only when `swap_counter_available=true`; the
Linux adapter counts render submissions, not compositor presentation. This
diagnostic requires a render counter and is not suitable for the current
Windows adapter, which does not expose one. Use a fresh fixture/output directory
to preserve earlier reports, and `VSF_DIAGNOSTIC_EXIT=1` for ordered shutdown.

## Closed-state memory sampling — task 003b

```sh
VSF_MEDIA_MEMORY="$PWD/tests/fixtures/generated" npm run tauri -- dev --features media-prototype -- --locked
```

This macOS diagnostic uses the same 4K HEVC fixture and keeps the desktop visible. It runs two warm-up open/seek/close cycles followed by 50 measured cycles in each of software and hardware-copy decoding. Each cycle waits for loaded state and a GPU swap, confirms the requested decoder at a five-second seek, then closes. It samples process RSS with `/bin/ps -o rss= -p PID` two seconds after the initial closed state, each mode's warm-up and every ten closes. Work and sampling run on a blocking worker, not the UI thread. Avoid app interaction, source edits and concurrent builds during the run.

`memory-results.txt` records all checkpoints and success/failure; `result=Ok(())` establishes completion of the lifecycle checks, not a memory-budget pass. RSS is reported in KiB for the app process alone; it excludes WebKit children, differs from physical footprint, and includes resident allocator/driver caches. Compare checkpoints within a warmed mode, not different process lifetimes. No memory-growth acceptance threshold is defined. Native operations remain uninterruptible, and this diagnostic has no hard overall deadline. It leaves media closed for external allocation inspection. `VSF_MEDIA_MEMORY` takes precedence over the other startup diagnostics.

## Native-only allocation investigation — task 003b

`tests/native/memory-probe.mm` includes the production Objective-C++ adapter in a standalone AppKit test app. It excludes Tauri, Rust coordination and WebKit, keeps a real GPU drawable, and compares recreating the player with reusing it via mpv `stop`/`loadfile`. Main owns the window/surface; one control worker creates/commands/destroys the player; the production renderer owns GL. Surface cleanup is acknowledged on main before the worker stops the app. The final join happens only after that cleanup. This is an isolation harness, not an alternate application architecture.

Build with the installed native compiler/mpv:

```sh
python3 scripts/build-memory-probe.py
```

The builder ad-hoc signs only the ignored diagnostic binary with the checked-in `get-task-allow` entitlement so Instruments can attach. It does not change application signing or system security. It targets the current SDK/host; it is not a minimum-macOS compatibility test.

Run cases sequentially in fresh processes with the desktop visible:

```sh
VSF_PROBE_THREADS=0 tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate no 50
VSF_PROBE_THREADS=0 tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" reuse no 50
VSF_PROBE_THREADS=4 tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate no 50
tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate auto-copy 50
```

The probe selects decoding before load, waits for a loaded frame/swap, seeks to five seconds and checks the actual decoder, then destroys or stops the player. State waits have 15-second deadlines, but blocking native calls do not have hard interruption bounds. Counts are limited to 1–500. Checkpoints after cycle 2 and each ten cycles wait two seconds, reporting bytes: physical footprint/resident size from `proc_pid_rusage`, and allocator live/reserved bytes from `malloc_zone_statistics`. Metrics have different accounting scopes and must not be subtracted as though they were interchangeable. `reuse` closed checkpoints still have a live core/render context; compare its final `destroyed` checkpoint too. A final `surface_released` sample follows actual graphics-context teardown. PASS means operations completed, not leak freedom or an accepted memory budget.

Optional diagnostic interventions:

- `VSF_PROBE_THREADS=4` sets `vd-lavc-threads` before loading. Unset uses the production adapter default (now four); set `VSF_PROBE_THREADS=0` to reproduce the original automatic-thread baseline.
- `VSF_PROBE_PLAY=1` adds a ten-second play sample per cycle, checking progress and reporting swaps/drop deltas. Use one cycle for this check.
- `VSF_PROBE_RELIEVE=1` requests `malloc_zone_pressure_relief` once after destroying the player, then resamples. This deliberately changes allocator behavior for diagnosis only; production never calls it.
- `VSF_PROBE_CYCLE_POOL=1` wraps each worker cycle in an Objective-C autorelease pool, drained before sampling. This tests whether temporary objects on the control thread account for growth.
- `VSF_PROBE_RECYCLE_SURFACE=1` (recreate mode only) releases the entire native surface/context after destroying each player and creates a new one on main. This isolates retention tied to the window-long GL context. It deliberately differs from application ownership and is not a proposed production policy by itself.
- `VSF_PROBE_ALTERNATE=/absolute/path/to/other.mp4` loads that fixture on even cycles. Both fixtures must support the five-second exact seek and requested decoder. Use this to check codec/resolution changes after a same-file reuse comparison.

For the 200-cycle hardware-copy isolation comparison, rebuild the probe and run these in separate processes sequentially, saving each output:

```sh
tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate auto-copy 200
VSF_PROBE_CYCLE_POOL=1 tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate auto-copy 200
VSF_PROBE_RECYCLE_SURFACE=1 tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate auto-copy 200
tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" reuse auto-copy 200
VSF_PROBE_ALTERNATE="$PWD/tests/fixtures/generated/h264-1080p.mp4" tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" reuse auto-copy 200
VSF_PROBE_ALTERNATE="$PWD/tests/fixtures/generated/h264-1080p.mp4" tests/fixtures/generated/memory-probe "$PWD/tests/fixtures/generated/hevc-4k.mp4" reuse no 200
```

Example allocation recording and exports (output paths must be new):

```sh
xcrun xctrace record --template Allocations --time-limit 60s \
  --output tests/fixtures/generated/native-allocations-debug.trace \
  --target-stdout tests/fixtures/generated/native-allocations-debug-output.txt \
  --launch -- "$PWD/tests/fixtures/generated/memory-probe" \
  "$PWD/tests/fixtures/generated/hevc-4k.mp4" recreate no 10
xcrun xctrace export --input tests/fixtures/generated/native-allocations-debug.trace --toc
xcrun xctrace export --input tests/fixtures/generated/native-allocations-debug.trace \
  --xpath '/trace-toc/run[@number="1"]/tracks/track[@name="Allocations"]/details/detail[@name="Statistics"]' \
  --output tests/fixtures/generated/native-allocation-statistics.xml
xcrun xctrace export --input tests/fixtures/generated/native-allocations-debug.trace \
  --xpath '/trace-toc/run[@number="1"]/tracks/track[@name="Allocations"]/details/detail[@name="Allocations List"]' \
  --output tests/fixtures/generated/native-allocation-list.xml
```

These track-detail exports were verified with Xcode/Instruments 27. The exported allocation list contained live entries at the final selected interval; it was not a complete list of all historical allocations. Persistent allocations can be caches or reachable resources, not necessarily leaks. Profiling perturbs the workload; do not compare instrumented footprint directly with uninstrumented runs. See task 003b for findings, confounders and raw artifact names.


## Decoder policy and extended desktop checks — 2026-10-02

Open requests now carry the decode preference before loading. The UI starts with prefer-hardware (`auto-copy`, allowing software fallback); software is selectable before Open and the same preference can be applied to the current session. The native PoC uses four software decoder threads. This is provisional tuning for the tested Mac, not a portable performance guarantee.

After building the feature-enabled debug app, run sequential fresh processes without concurrent encodes/builds or UI commands:

```sh
VSF_DIAGNOSTIC_EXIT=1 VSF_MEDIA_SMOKE="$PWD/tests/fixtures/generated/vfr-offset.mkv" target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
VSF_DIAGNOSTIC_EXIT=1 VSF_PLAYBACK_BENCH="$PWD/tests/fixtures/generated" target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
VSF_DIAGNOSTIC_EXIT=1 VSF_MEDIA_MEMORY="$PWD/tests/fixtures/generated" VSF_MEMORY_MODE=hardware VSF_MEMORY_CYCLES=200 target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
VSF_DIAGNOSTIC_EXIT=1 VSF_MEDIA_MEMORY="$PWD/tests/fixtures/generated" VSF_MEMORY_MODE=software VSF_MEMORY_CYCLES=200 target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
VSF_DIAGNOSTIC_EXIT=1 VSF_MEDIA_SOAK="$PWD/tests/fixtures/generated" target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
```

`VSF_DIAGNOSTIC_EXIT` uses the existing orderly worker/render/AppKit cleanup before exiting, allowing sequential automation without a Vite watcher. Memory counts are bounded to 1–500, default 50. Omit the mode to exercise both in the original order; separate processes are needed for a fairer comparison. Reports are now `memory-{hardware,software,both}-results.txt`, and include final `vmmap -summary` output. RSS checkpoints exclude WebKit XPC processes; physical footprint is a different accounting metric. Neither establishes a whole-application budget or leak freedom.

Generate additional fixtures with `python3 scripts/extended-playback-fixtures.py`. It records complete commands, versions and ffprobe metadata in `extended-generation.json`: 180-second H.264/25 fps with a ten-second GOP in Matroska; 90-second 1080p HEVC Main10/30 fps with BT.2020/PQ tags; and 30-second FFV1/24 fps for hardware-unavailable fallback. PQ-tagged synthetic content tests the decode/tone-mapping path, not mastered HDR quality. Generation is bounded per command and overwrites only its named outputs.

The extended runner also expects `generated/big-buck-bunny.mp4`: the unmodified 320×180, 24 fps, 596.458-second H.264/AAC movie from [Blender's official archive](https://download.blender.org/peach/bigbuckbunny_movies/BigBuckBunny_320x180.mp4.zip). Attribution: **(c) copyright 2008, Blender Foundation / www.bigbuckbunny.org**. Licensed under [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/) per the [project's attribution instructions](https://peach.blender.org/about/); downloaded unchanged, retained locally in ignored fixtures, with the original credits. `bbb-provenance.json` records source and SHA-256 hashes. This low-resolution real movie broadens content/container/duration coverage. The additional unmodified 1920×1080 H.264/AAC MOV from the same official archive adds higher-resolution real content and MOV coverage (60 seconds per decoder). Fetch either variant reproducibly with `python3 scripts/fetch-reference-movie.py --resolution 180` or `--resolution 1080`; the script extracts only a fixed expected archive member and records provenance/hashes. The 1080 archive is about 725 MB. Both variants carry the attribution above.

```sh
VSF_DIAGNOSTIC_EXIT=1 VSF_EXTENDED_PLAYBACK="$PWD/tests/fixtures/generated" target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
```

This opt-in runner takes about 18 minutes with both reference resolutions. It requires automatic FFV1 fallback, checks decoder identity, seeks near the end/middle/start, checks forward/backward frame round trips, watches for two-second position stalls, and plays the movie for 580 seconds. Known frame rates come from these fixtures only. `extended-results.txt` reports seeks, swaps, drop deltas and sampled mpv `avsync`; the latter is a clock observation, not an audible-sync or physical-presentation measurement. State/lifecycle failures are fatal; drop/sync metrics are observations to assess separately, so an `Ok(())` alone is not a quality-budget pass. Native calls remain uninterruptible. The extended runner takes precedence over other startup diagnostics. Set `VSF_EXTENDED_FILTER=long-gop-h264.mkv` or another exact fixture name to repeat one file; the filtered report is `extended-filter-results.txt` and should be copied to a distinct evidence name before another filtered run.

## macOS packaging and transparency probes

`python3 scripts/package-macos-probe.py --output tests/fixtures/generated/portable-final/VisualSubForge.app` creates a new local copy of the debug app, recursively copies absolute non-system Mach-O dependencies, rewrites them to the bundle's Frameworks directory, ad-hoc signs and verifies the result. It refuses existing output, unresolved loader tokens and conflicting library names. `dependency-manifest.json` records original hashes/paths and build minimums. The output Info.plist minimum is raised to the highest discovered dependency minimum. This is a host-built arm64 experiment, not a distributable release: it does not package dynamically discovered plugins, font configuration/assets or the analysis CLI tools, collect all license notices/sources, notarize, or validate a clean machine. The source bundle and installed Homebrew files are untouched.

The transparency probe is isolated from the app. Build `tests/native/webkit-transparency-probe.mm` with `xcrun clang++ -std=c++17 -fobjc-arc ... -framework AppKit -framework WebKit -o ...` on the current SDK. It compares clear CSS plus public `underPageBackgroundColor` against the same setup with private `drawsBackground` disabled, over a green native view. It terminates after 120 seconds. On the development Mac only the private-API panel revealed green. This does not establish that every alternative composition design requires private APIs.


The broad playback/stress copy is `generated/portable-verified/VisualSubForge.app`; the final numeric-boundary correction is also included in `generated/portable-final-check/VisualSubForge.app`, which passed the Homebrew-read-denied smoke recorded in task 003b. Replace the executable path in the diagnostic commands with this copy and launch with `env PATH=/usr/bin:/bin` to avoid reliance on Homebrew executables. `DYLD_PRINT_LIBRARIES=1` on the smoke run records actual loaded dylib paths; this checks the exercised path, not every plugin or resource the dependencies might load in other workflows. A clean OS/architecture test remains necessary.


The native numeric probe also checks that unavailable properties remain distinct from signed values: a missing property returns NaN through the C boundary (None in Rust), while an explicitly set `audio-delay=-1` is preserved and reset to zero before loading. Final numeric checks under a locked desktop establish property/lifecycle behavior, not visible rendering quality. For presentation measurements, keep the desktop unlocked; the October 2 later packaged runs encountered a locked Mac and need a foreground repeat.


## Editing workflow fixture — task 004

`editing.srt` is original synthetic text paired with `vfr-offset.mkv` / `cfr-offset.mkv`: four cues cover the 0.250 s audio start, 1.250 s positive pulse, 2.000 s second-track negative pulse and a final multiline cue. It deliberately includes Unicode, literal markup and numeric dialogue. No legacy content is copied. For desktop editing, copy it to ignored `generated/editing-workflow.srt` so save/reopen tests do not modify the reference fixture.

```sh
cp tests/fixtures/editing.srt tests/fixtures/generated/editing-workflow.srt
```

Desktop checklist: open media and subtitles through native menus; compare each selected audio track's waveform pulse; seek a cue; edit text/start/end and Apply; undo/redo; Save As to another generated SRT and reopen; reject an invalid time; cancel replacement/quit with a draft; save/discard deliberately; resize all three splitters and the window while video is paused/playing; zoom/pan/follow/drag a waveform range and loop it; close media and verify black native video. `long.flac` exercises a two-hour viewport and cancellation by immediate replacement. These new interface checks are still pending an unlocked desktop.


Desktop results for the editing checklist are recorded in [004 evidence](../../docs/tasks/004-desktop-evidence.md). The native Quit data-loss and pan-slider issues were fixed and retested. The two-hour audio-only subsecond-loop clock check failed and is retained as [task 005](../../docs/tasks/005-audio-loop-timing.md), with a standalone `tests/native/audio-loop-probe.py` reproduction; do not count that as a passed precision-timing test.

## Binary frame identity fixture — libVLC seek investigation

`scripts/seek-frame-fixture.py` creates original synthetic H.264/AAC media in a new directory: 640×360, 30 fps, 360 frames, 12 seconds. Twelve 16×32 cells encode white/black anchors and a ten-bit little-endian frame ordinal. It needs Python's standard library and FFmpeg with libx264/AAC, with no font or NumPy dependency. Generation records the command, FFmpeg version, output hash and generator hash; byte-identical output across FFmpeg versions is not promised.

```sh
python3 scripts/seek-frame-fixture.py --output tests/fixtures/generated/seek-frames
```

Build `tests/native/libvlc-paused-seek-probe.c` against the matching pinned SDK using the [audit's command](../../docs/libvlc-feasibility.md#output-isolation-and-standalone-reproducer). The optional snapshot argument requires an existing directory. After the process exits, independently check its image files:

```sh
mkdir tests/fixtures/generated/seek-case
VLC_PLUGIN_PATH="$VSF_VLC_SDK/lib/vlc/plugins" "$VSF_SEEK_PROBE" \
  "$PWD/tests/fixtures/generated/seek-frames/frame-id.mp4" step-after-seek \
  "$PWD/tests/fixtures/generated/seek-case" \
  > tests/fixtures/generated/seek-case/stdout.log \
  2> tests/fixtures/generated/seek-case/stderr.log
python3 tests/native/check-seek-frame-id.py \
  --fixture tests/fixtures/generated/seek-frames/frame-id.mp4 \
  --case tests/fixtures/generated/seek-case \
  --output tests/fixtures/generated/seek-case/identity.json
```

The checker verifies every source marker and rational PTS, then identifies snapshot cells without OCR, target-biased image matching, or a timing tolerance. Its explicit diagnostic contract selects the frame covering the requested time in `[PTS, next PTS)`. Missing images, ambiguous/nonuniform cells and invalid anchors fail. The probe's coarse position exit status is separate from this exact-image verdict; on the tested SDK, the command sequence above passes the former and fails the latter. `step-after-seek` is an investigation control, not an adopted application fix. These are engine snapshots using dummy output, not physical display, GPU or audio validation. See the [current results](../../docs/libvlc-feasibility.md#regenerable-binary-marker-regression).

The later [experimental core patch](../native/libvlc-seek/README.md) passes the exact-image check with ordinary `plain` seeks on this fixture. Its recipe explicitly links the patched core and matching SDK plugins; the SDK-only command above retains the baseline failure. `resume-after-seek` takes the same snapshots, then checks reported playback progression and a return to pause after each seek. It rejects progress more than 250 ms ahead of elapsed wall time; this catches gross jumps without establishing an audio/device precision budget. Separate step/previous/resume experiments still fail, so the patch is not ready for integration. Keep each run's output in a fresh directory.

## Scene-cut fixtures — 008

```sh
python3 scripts/scene-fixtures.py
cargo test --locked -p media-analysis --test scenes -- --ignored
```

FFmpeg generates black/white/black frames without external media, with a five-second
container origin and expected project cuts at 1/2 seconds. The VFR variant retains
cut frames and irregularly drops others. Flat-video and audio-only cases test
empty and unsupported inputs. VSF_FFMPEG/VSF_FFPROBE override executable discovery.

### Scene detail and decoder scaling follow-up

`python3 scripts/scene-fixtures.py` also creates a fixed-GOP H.264 fixture with
B-frames and real cuts at 1.12/2.24 seconds outside keyframes, plus a portrait
continuous-motion negative fixture. The five opt-in scene tests exercise both
detail settings on CFR/VFR origins, reduced-detail non-keyframe cuts, portrait
motion, flat video, audio-only rejection, cancellation and deadlines.

For local measurements without exporting decoded pixels:

```sh
python3 scripts/scene-benchmark.py /path/to/video --seconds 60 --repeats 3 --output tests/fixtures/generated/scene-benchmark.json
# Use --seconds 0 for the entire input, or --cases to select named pipelines.
```

The script records tool versions, metadata, timings and cut timestamps. OS cache
and background activity are uncontrolled. See task 008's performance note for
actual commands, results and accuracy limits.

## Styled resync fixtures — Task020

`subtitles/resync.ass`, `.ssa` and `.srt` are original fixtures derived from
Task016 style/opaque-record controls and Task018's original two-glyph embedded
font. No external fonts are added. Two cues at 3–4s and 4.1–5s fit the synthetic
resync corpus's independently known matching intervals. The first cue has a
`\k20` tag; the second is static. Offset output is 6–7s and 7.1–8s. A measured
0.96 speed map refuses the karaoke row; removing that tag in the static control
yields 1.960–2.920s and 3.020–3.880s in ASS/SSA (3.016s SRT second start).
CLI and desktop tests compare tags, source protection, opaque records, history
and output bytes. The font's existing generation/license instructions still apply.

The 10k resync benchmark uses Task016's full fixture with only timestamps halved
to fit the existing four-hour analysis limit; it records both parent/derived
SHA256. All cues, text, styles and unknown records remain. CPU/RSS budgets remain
500ms p95 / 96MiB additional sampled RSS. Original generated files stay unchanged.
