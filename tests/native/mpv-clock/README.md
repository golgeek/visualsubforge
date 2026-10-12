# Isolated mpv audio-clock experiment

The three additional corrections approved for audio completion now have an
explicit [`--audio-candidates` recipe](candidates/README.md), separate from the
ten-patch baseline below. Their application integration is capability-gated;
platform/content validation remains in progress. For the complete local Mac
build and launch path, see [the build guide](../../../docs/build-macos.md).

Dependency work for [task 005](../../../docs/tasks/005-audio-loop-timing.md) and [decision 0007](../../../docs/decisions/0007-audio-transport-clock.md). The user approved the bounded VSF-maintained variant on 2026-10-05. This accepts maintenance ownership; the audio candidates still require validation before production integration or release adoption.

The ordered patches in [series.json](series.json) apply to official upstream `master` commit `c1529642089bfebfc928a1c1664638a7a5d219ba`, selected on 2026-10-05. Builds use this exact commit, not a moving branch. The preparation script pins the manifest hash and verifies every patch hash before applying the series to a new isolated clone. It does not copy uncommitted files, install anything, or modify the supplied checkout. Failed preparation leaves its output for inspection; use another new output directory after correcting the cause.

```sh
# Obtain source if it is not already available:
git clone https://github.com/mpv-player/mpv.git /tmp/mpv-upstream
# Or supply the existing external investigation clone as --source.
python3 scripts/prepare-mpv-clock.py --source /tmp/mpv-upstream --output /tmp/vsf-mpv-clock-prepared
```

On the development Mac, with Homebrew's dependencies and the isolated Meson 1.12.1/Ninja 1.13.2 tools available:

```sh
PATH=/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH meson setup /tmp/vsf-mpv-clock-prepared/build /tmp/vsf-mpv-clock-prepared/source -Dlibmpv=true -Dcocoa=enabled -Dswift-build=enabled -Dmacos-media-player=disabled -Dtests=true -Dmanpage-build=disabled -Dvapoursynth=disabled
PATH=/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH meson compile -C /tmp/vsf-mpv-clock-prepared/build -j 6 mpv:executable
PATH=/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH meson test -C /tmp/vsf-mpv-clock-prepared/build --print-errorlogs -j 6
python3 tests/native/audio-boundary-probe.py --mpv /tmp/vsf-mpv-clock-prepared/build/mpv
```

Build commands are macOS-specific and do not pin the entire transitive toolchain. Optional VapourSynth is disabled because the development environment lacks its required `VSScript4.h` header; this recipe does not validate that filter. Use the normal Cocoa/Swift configuration: the earlier minimal build omitted a utility required by enabled Mac audio backends. Compile the executable/test targets explicitly; the local optional manual-page tool is incompatible. No `meson install` step is intended.

The pinned base already contains the [upstream rendering correction](../mpv-render/README.md). The manifest retains its provenance and the script verifies commit ancestry instead of applying a redundant backport. `--render-only` applies the shared Mac build guard described there while omitting the audio patches.

The default recipe also applies the separate [CoreAudio cleanup series](../mpv-coreaudio-cleanup/README.md): a tested local handle-ownership follow-up, with its two prerequisite fixes already in the upstream base. These do not change the queued-clock capability marker or timing policy. `--render-only` excludes both audio-related series.

The nine audio patches separate the changes for review. Patch 0005 now uses `0005-pcm-sample-boundaries.patch`, preserving upstream’s PCM-only guard. The older `0005-nearest-sample-boundaries.patch` is retained only for historical v0.41.0 evidence and is not applied. Apply them in order; a partial series is not the tested VSF dependency.

Patch 0008 fixes the push-output EOF busy loop introduced by patch 0002 while retaining deadline scheduling for consumed pull queues. Mac, Windows and Ubuntu checks are recorded in task 005. It changes no timestamp calculation and does not resolve the PipeWire presentation-clock gate.

Patch 0009 corrects a CoreAudio property/type mismatch: an `AudioChannelLayout` was passed to the channel-map property, which expects an integer array. The corrected input-layout property restores mono initialization without forcing stereo. This is a Mac-only dependency change, validated on the current device; Bluetooth, multichannel and other devices remain unverified.

A separate [queued-EOF pause candidate](resume-queued-eof-candidate.patch), applied after the nine-patch series, remains under investigation. Event-driven tests expose AVFoundation pause/resume failure after its source queue is consumed. The candidate restores completion and captured audio after Resume, but capture continuity remains unresolved; it is not in the ordered series.

A standalone [AVFoundation pause probe](../avfoundation-pause-probe.m) reproduces a captured source discontinuity without mpv on the development Mac. Explicit time anchoring and 10/100/250 ms buffers do not remove it. Task 005 retains capture scope and reproduction commands; this does not prove acoustic loss on every device.

The standalone probe accepts an optional fifth argument, `default|varispeed|time-domain|spectral`, selecting `audioTimePitchAlgorithm` before enqueue/start and reporting the actual selected algorithm in its READY line. Omitting it preserves the prior default. Normal-speed pause captures reject algorithm selection as a continuity remedy: Varispeed retains the gap and Spectral increases it. These are native diagnostic controls, not a change to mpv or application speed/pitch policy. No renderer flush/replay or empirical clock offset is introduced.

| Patch | Scope and reproduction |
| --- | --- |
| 0001 | Queued timeline and unit tests; `audio-loop-probe.py --capture-matrix` exposes the original short-loop discrepancy |
| 0002 | Pull-output pause/drain, clock availability through decoder restarts, immediate user pause; `audio-continuation-probe.py --transport-matrix` exposes unavailable time and the EOF pause stall |
| 0003 | AVFoundation scheduling across underruns; continuation and EOF captures retain the source/tone sequence |
| 0004 | Seekable output replacement; `audio-clock-lifecycle-probe.py` checks output/clock recovery after channel changes |
| 0005 | Nearest-sample trimming and unit tests; `audio-boundary-probe.py` compares four exact PCM repeats at four rates |
| 0006 | Local capability and read-only property test; keep separate from potential upstream changes |
| 0007 | Preserve exact-seek loop eligibility at restart despite drained/rounded endpoints; PCM repeat oracle plus `audio-loop-seek-regression.py` covers seeks past B, before A and return to looping |
| 0008 | Bound EOF wakeups to the pending pull-output deadline; direct ALSA before/after traces expose the original zero-delay polling storm |
| 0009 | Use the AudioChannelLayout input property for its matching struct; mono CoreAudio initialization, default-backend captures and stereo channel identity pass on the development Mac |

Task 005 records full invocations, before/after evidence and limitations. This organization makes the dependency reviewable. Bounded maintenance ownership is approved, while functional acceptance and release adoption remain open. No upstream submission has been sent.

Application integration detects the local read-only `vsf-audio-clock-version` property with value `1`. This is an experiment marker, not an upstream API. Selecting the library enables queued audio-only time and flush-on-clear; an ordinary unpatched library retains the legacy path. The property test checks its value and that it cannot be changed. The preparation script pins the complete series, including this marker/test.

After building VSF with `media-prototype` and generating `long.flac`, the application transport regression is:

```sh
DYLD_LIBRARY_PATH=/tmp/vsf-mpv-clock-prepared/build DYLD_PRINT_LIBRARIES=1 VSF_AUDIO_SMOKE="$PWD/tests/fixtures/generated/long.flac" VSF_DIAGNOSTIC_EXIT=1 target/debug/visualsubforge
```

It fails explicitly when the capability is absent. Its passing contract covers short-loop clock progression, pause, outside-range seeks, clear in both pause states, EOF cursor and close. It does not validate acoustic latency or replace the visible Follow check. For manual desktop checks, build the `.app` bundle and run its `Contents/MacOS/visualsubforge` executable with the same library environment, omitting the two `VSF_*` diagnostic variables; open the fixture normally. Playback Diagnostics must report `queued timeline (experimental)`. The environment applies to that process only, never the Homebrew installation.

The modified upstream C/Objective-C sources retain their LGPL-2.1-or-later notices. New experimental source files use the same SPDX identifier for compatibility. [LICENSE.LGPL](LICENSE.LGPL) is copied from the pinned upstream checkout. This covers the patch's source changes, not the complete linked dependency graph or VSF's still-undecided project license. No VSS legacy source is included.

`python3 tests/native/audio-eof-regression.py --mpv <player> --ao <backend>` checks ordinary EOF, pause/resume, exact seek and replacement through pause/restart events, without periodic clock polling. Add `--action-seconds .95` for a later transition in its one-second fixture. It defaults to explicit stereo; `--audio-channels mono` exercises mono initialization. It uses a bounded watchdog. The nine-patch series still fails the AVFoundation queued-EOF pause case; this regression records an open defect rather than declaring the dependency accepted.

Add `--paused-reset` to also seek to 0.5 seconds or load a distinct two-second file while paused, then resume after 250 ms. These cases verify retained pause state, actual file identity, the new duration/transport position and completion against the new source's deadline. Python checks file identity because mpv can normalize Windows path separators; the raw reported path remains in the trace. These are event/state checks, not captured continuity or acoustic timing. `python3 tests/native/audio-eof-regression-test.py` runs negative controls for wrong source/state, missing/duplicate reset and stale or missing completion. The separate pause candidate is still outside the ordered series.

An Ubuntu ARM64 build also uses the isolated source, with the guest's development dependencies:

```sh
meson setup prepared/build prepared/source -Dlibmpv=true -Dtests=true -Dmanpage-build=disabled
meson compile -C prepared/build -j 6 mpv:executable mpv:shared_library
meson test -C prepared/build --print-errorlogs -j 6
python3 tests/native/audio-boundary-probe.py --mpv prepared/build/mpv --output-dir tests/fixtures/generated/pcm-evidence
python3 tests/native/audio-loop-seek-regression.py --mpv "$PWD/prepared/build/mpv"
```

The optional boundary output directory retains source PCM, output and verbose logs; it does not weaken the exact four-repeat comparison. The seek regression uses null output and therefore makes no audible-output claim. Both checks pass after patch 0007 on the development Mac and Ubuntu VM. The separate `audio-transport-regression.py --mpv ... --ao null|pulse|pipewire` checks clock properties and preserves complete traces; its strict monotonic assertion still exposes Pulse/PipeWire backward steps that need investigation. See task 005 for exact environments, failures and evidence.

Remaining gates include the documented AVFoundation seek/restart gap, acoustic limits, device/non-seekable/passthrough behavior and Windows/Linux playback validation. The bounded maintenance strategy is approved; its candidate implementation still needs acceptance. VM evidence must remain separate from physical device and x86-64 claims.

### Ordinary/pause content discontinuity check

`audio-fingerprint.py` promotes the unique-signal correspondence helper from the
EOF investigation into a maintained offline diagnostic. It accepts mono/stereo
48 kHz PCM16 reference WAV and the stereo float32 output plus adjacent `.json`
metadata from `process-audio-capture`. NumPy is required. For example:

```sh
python3 tests/native/audio-fingerprint-test.py
python3 tests/native/audio-fingerprint.py REFERENCE.wav CAPTURE.f32 --output NEW_REPORT.json
```

The report records exact input/analyzer hashes and refuses to overwrite an
existing file. Exit 1 rejects insufficient correspondence or a gross forward or
backward source discontinuity; malformed data also fails. Exit 0 means neither
was detected within matched content. Every mapping transition is checked,
including short gaps and overlapping windows; the old layout-specific analyzer
only checked gaps longer than 100 ms. Timestamp validity and complete capture
block coverage are required. Stereo is compared with the corresponding source
channel; mono is compared with both captured channels.

The existing 256-frame windows, 64-frame hops, 0.97 correlation, 0.10 uniqueness
margin and 500-window minimum remain explicit. A two-window allowance and all
nonzero unmatched samples conservatively accommodate uncertain edges. This is
a gross-loss detector, not a precision budget or sample-conservation proof.
Unmatched source beginnings/endings remain visible in the report. It does not
verify that a requested pause occurred, replace the original channel-isolation
check, or validate audible presentation. Pair it with command/state evidence.
Intentional seeks and loop wraps need their own expected source mapping; do not
apply this ordinary/pause oracle to those transitions. Synthetic controls cover
deletion with no/short/long silence, replayed content, wrong-channel noise,
silence, insufficient coverage and invalid capture timestamps.

For Linux driver controls, add `--ordinary` or `--ordinary-refresh` to the transport regression. With `--ao pulse`, optional `--capture-pulse` records only the test process's sink input using the executable `tests/native/pulse-stream-capture.py` helper and installed `pactl`/`parec`. It checks the three-tone content sequence, not presentation timing: capture metadata deliberately labels read-receipt times and must not be passed to the macOS tap timing analyzer. Missing/ambiguous player streams fail without whole-sink capture. Task 005 records passing content captures alongside the still-failing strict clock comparisons.

For Windows clock observations, use `audio-transport-regression.py --mpv <player.exe> --ao wasapi --observer lua` with `audio-transport-observer.lua` beside the Python helper. `--ordinary` and `--ordinary-refresh` use the same assertions as POSIX tests. The opt-in Lua observer requires a Lua-enabled mpv and exercises its client APIs rather than named-pipe/application routing; Python bounds the child lifetime and retains raw/normalized traces and native logs. `--capture-pulse` remains POSIX-only. The Windows ARM64 candidate passes the null/WASAPI matrix while stock reproduces the loop error; task 005 records exact staging, timing limits and outstanding output-capture/lifetime gates.

For Windows process-only capture, compile `tests/native/windows-process-capture.cpp` with the guest CLANGARM64 toolchain (`clang++ -std=c++17 -Wall -Wextra -Werror -municode windows-process-capture.cpp -o windows-process-capture.exe -lole32 -lmmdevapi -luuid`) and add `--capture-wasapi <absolute-helper.exe>` to the Lua/WASAPI transport command. Keep `audio-tone-content.py` beside the observer: both Windows and Pulse capture now use that shared content oracle. The helper records only the specified process tree using the public process-loopback API; no whole-mix fallback, microphone or default-device change is used. Analyze a retained result directory with `python windows-capture-analysis.py <directory>` for packet/QPC observations and explicit clock-alignment uncertainty. Captured continuation passes, but a repeated Windows loop-clear capture contains a 12 ms quiet interval, and capture-versus-clock residuals remain unresolved. These observations do not establish acoustic timing.


The outside-range regression also runs on Windows with `--observer lua --ao null|wasapi`; all six candidate cases pass in the supplied ARM64 VM. Opt-in repeated `--mpv-option=--name=value` arguments in the transport helper support diagnostic player logging without changing normal test policy. Task 005 records buffer-level Windows timing findings and the separate GGML dependency-unload defect. Neither is hidden by changing the clock tolerance or the seven-patch series.

### Windows capture-clock measurement

The separate [WASAPI diagnostic patch](../wasapi-clock-bracket.patch) applies after the nine-patch series. It changes only `audio/out/ao_wasapi.c`: it logs existing device-delay/feed observations and brackets `mp_time_ns()` with QPC reads eight times at initialization and teardown. This is instrumentation, not another clock correction or a member of `series.json`. Feed logging can affect scheduling; do not ship this diagnostic.

On the Windows CLANGARM64 development toolchain, the isolated executable configuration is `meson setup prepared/build prepared/source -Dlibmpv=false -Dtests=true -Dmanpage-build=disabled`, followed by `meson compile -C prepared/build -j 6 mpv:executable` and `meson test -C prepared/build --print-errorlogs -j 6`. This configuration runs 11 applicable tests, not the libmpv lifetime suite.

Run the process-only capture command above with that executable and `--mpv-option=--msg-level=ao/wasapi=trace`; then analyze its result with `python windows-capture-analysis.py RESULT_DIRECTORY --native-clock`. The analyzer requires startup/teardown brackets to intersect and agree with the independent subprocess calibration. It aligns reads of the same QPC clock; it does not fit device drift or infer acoustic latency. Lua observations now include property-read completion times, with their one-microsecond timer resolution retained in the analysis. Older traces retain the conservative next-callback fallback. `python tests/native/windows-capture-analysis-test.py` checks bracket invariants without a Windows device. Task 005 records the six native captures and the still-positive capture/clock discrepancy.


### Steady-loop captured content

`audio-loop-content.py` applies the same unique-signal matcher to an explicitly
selected steady 1× interval. Supply integer 48 kHz source A/B and capture frame
bounds, justified by the retained command timestamps. Exclude startup, pauses,
seeks and loop-clear transitions; this tool does not infer their completion.

```sh
python3 tests/native/audio-loop-content-test.py
python3 tests/native/audio-loop-content.py REFERENCE.wav CAPTURE.f32 --loop 36000 51504 --interval 96000 192000 --output NEW_REPORT.json
```

The oracle requires at least 500 matched windows and two observed B-to-A wraps.
It checks content against the loop bounds, only unwraps verified near-boundary
transitions, and rejects unexplained loss/replay or interior intervals longer
than two windows without verified correspondence. A missing-match interval
straddling a possible wrap is **unverified**, not evidence of replay. The report
retains those transitions and unverified capture edges. It rejects both channels
independently and hashes the actual inputs and both analyzer modules.

Exit 1 may mean insufficient evidence, not a proven player defect. In particular,
resampling or altered content can reduce matcher confidence. The one-window
range allowance and two-window transition allowance are conservative diagnostic
thresholds, not sample precision or an accepted product timing budget. Capture
is process output, not acoustic/device presentation. Task 005 records the fresh
CoreAudio and retained VLC comparisons, including failures and uncertainty.


Add `--fractional` to the steady-loop command to fit a positive blend of adjacent
reference samples for each window. This accommodates a fractional sample offset
without lowering the correlation/uniqueness thresholds or fitting an arbitrary
time warp. Gain and DC offset are nuisance terms; phase is bounded to [0, 1].
The global best and competing source positions both use this model. Source frame
positions may then be fractional. Integer matching remains the default, and both
modes retain the same loop/coverage/gap requirements. Synthetic controls check
known phases, small rate differences, ambiguous/unrelated/inverted content and
corrupted resampled loops. This fit supports correspondence; it does not identify
a device's resampler implementation or prove sample identity.


### Loop-clear captured content

`audio-clear-content.py` consumes a unique reference WAV, process capture and the
`audio-loop-probe.py` trace containing exactly one successful
`diagnostic-clear-loop-at-audio-pts` action at 1× speed:

```sh
python3 tests/native/audio-clear-content-test.py
python3 tests/native/audio-clear-content.py REFERENCE.wav CAPTURE.f32 TRACE.json --output NEW_REPORT.json
```

It converts recorded command bounds to capture frames using host timestamps,
selects a high-confidence anchor 100 ms before the command, and requires a
phase that cannot naturally wrap before clear finishes. An unsuitable phase is
unverified; the analyzer never moves the anchor after clear. Fractional matching
checks source continuity through the reference EOF with the existing two-window
gross-discontinuity allowance. Source continuation beyond B, adequate coverage
and interior correspondence are required. Exact-zero gaps are measured separately:
preserving content across silence does not establish seamless playback. Input,
trace and analyzer hashes are retained; existing reports are never overwritten.
This is a process-capture oracle, not an acoustic/device-presentation verdict.

Correlation excludes constant reference windows, whose normalized correlation
is undefined. A direct transition count guards against cumulative-variance and
FFT roundoff making a silent or constant window beat the real signal. The
fingerprint regression includes long zero and nonzero constant regions with
both integer and fractional matching. This does not lower match thresholds or
make intentional pause intervals evidence of content continuity. Task005's
`float-clear-states-v1` retains the failing pre-fix control and separate native
zero-callback accounting for paused playback.
