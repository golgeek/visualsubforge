# Native reliability follow-up

Stage 8 is in progress. This record covers individual corrections; the remaining
audio, gesture, dialog/lifetime and distribution work in the
[roadmap](feature-roadmap.md) and [runtime gates](native-runtime.md#remaining-adoption-gates)
still applies. Physical Windows/Linux installation and GPU/audio acceptance remain
separate release gates.

## PCM WAV waveform origin — 2026-10-18

The earlier native legacy-import run `desktop-PSOVDS` loaded a raw PCM WAV and
restored paused playback at 1.250s, but waveform analysis reported “Container start
time is unknown.” A new actual-FFmpeg test reproduces this failure before the fix;
the original screenshot and failing test log are retained.

`media-analysis` now verifies a fallback only when FFprobe reports a WAV container,
exactly one stream, a `pcm_` codec and no container start time. A second probe reads
one packet and requires its stream index to match and its explicit PTS to be zero.
Known origins keep their original value; invalid/nonfinite origins, other unknown
formats, non-PCM codecs, multiple streams and ambiguous packets remain refused.
Broadcast-WAV recording `time_reference` metadata does not replace playback PTS.

The existing off-UI subprocess supervisor owns both probes and decoding. The extra
probe shares cancellation, the remaining overall deadline, 64 KiB output bound and
kill/reap cleanup. No worker, IPC, native dependency or document/history owner was
added. Playback, scene-origin policy and source files are unchanged.

Independently generated ordinary WAV and BWF fixtures use four seconds of 8 kHz
`pcm_s16le`, a pulse at 1.25s and first-packet PTS 0. BWF has nonzero recording
`time_reference=480000`. Both produce exactly 32000 samples/400 peaks and the pulse
in bucket 125. Existing CFR/VFR offset/track selection, two-hour sample mapping,
invalid-media and cancellation integrations remain passing.

| Environment | FFmpeg/FFprobe | Media-analysis unit tests | Waveform integrations |
| --- | --- | ---: | ---: |
| Mac ARM64 | Homebrew 9.0.2 | 41 passed | 5 passed |
| Ubuntu 26.04 ARM64 VM | 8.0.1-3ubuntu2 | 41 passed | 5 passed |
| Windows 11 ARM64 VM | MSYS2 clangarm64 9.0.2 | 40 passed | 5 passed |

The additional Unix-only test waits for a real blocked packet probe, then verifies
cancellation and timeout both reap it. All platforms test origin/packet refusal
rules. The ordinary media-analysis command initially ignores 18 integration tests;
the five waveform tests are then explicitly run. Unchanged scene/visual integrations
were not rerun. Zero-test harnesses are excluded from the table.

The full Mac workspace passes 507 tests/33 ignored, strict Clippy, Rustfmt, ESLint,
Prettier and the strict frontend/native build. Guests pass focused strict Clippy
and Rustfmt. Commands, with the pinned toolchains and explicit guest tool paths:

```sh
python3 scripts/media-fixtures.py
cargo test --locked -p media-analysis -- --test-threads=1
cargo test --locked -p media-analysis --test ffmpeg -- --ignored --nocapture --test-threads=1
cargo clippy --locked -p media-analysis --all-targets -- -D warnings
```

The complete Mac native `--legacy-import` workflow passes all four subtitle formats
and WAV restoration in `test-results/desktop-tEfha9/`, including original-file
preservation, edit/history/save/reopen/recovery, cancellation and normal exit 0.
Its WAV case now requires no analysis/error placeholder and actual painted waveform
pixels. An independent CUA screenshot confirms the waveform at the restored 1.250s
position. This is waveform/display evidence, not speaker-output acceptance or a
new guest-native/GPU claim. Supplied file choices and DOM events remain distinct
from real chooser and physical-input acceptance.

Local evidence is under `test-results/native-reliability-20261018/`: the retained
`wav-regression-before.log`, unit/integration/workspace/Clippy logs, native build/run
logs, source patch, binary identity and native observation. The fresh live-process
receipt records 46 mapped backing images with none unavailable and verifies:

- Desktop SHA256 `2c431dabf75712b4b626e78e7470fb11aee438b468b8dcb42185842b351c668e`.
- Sibling CLI SHA256 `a258fa9b17f62f14ac2d5eb62669331e33dd496ad4feedc8d047166157c5cd65`.
- Private libmpv SHA256 `fd572d9e18640414d3dd480ae6d2fad5b166cdbd9377ac65009b527e17d96a0f`.

The build reuses the [Mac native recipe](build-macos.md) and isolated
`test-results/plugin-layout-native-build/target` with `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_BUILD_JOBS=4`, features
`desktop-e2e,media-prototype`, and the existing plugin-native test configuration
(same permissions as `tests/desktop/tauri.test.json`, separate app identifier).
Run `node tests/desktop/run.mjs --legacy-import --binary /absolute/path/to/native/app`
with that recipe's process-local runtime. The optional native observation pause
uses `VSF_LEGACY_IMPORT_CAPTURE=1` and
`VSF_LEGACY_IMPORT_CAPTURE_LABELS=import-audio-restored`.

Guest receipts, tool/fixture hashes, exact commands and generation logs are the
`*-wav-*` files under `test-results/plugin-platform-20261018/`. Both verify 790 source
hashes from base `91e6ee1` plus this correction; source archive SHA256 is
`7a18c4ca966b6c865b06cbbb683c07fa82e895647fe251a582ab5a43f9f7b533`.
Windows returned to its original paused state and idle-pausing preference.

This closes the reproduced raw-WAV waveform defect within the stated scope.
Audio loop/clear discontinuities, Linux clocks/outputs, gesture interruption,
dialog/resource lifecycle, current CI/distribution and identifiable physical-device
acceptance work remain required. Stage 8 and the umbrella goal stay open.

## Current CoreAudio clear-boundary audit — 2026-10-18

Fresh captures now exercise the current Rust `media_transport::clear_loop`
against the selected candidate, including clears near B while playing and paused.
This is a component/content audit; it does not close the full editor, changing-rate,
replacement, other-output or physical presentation gates.

Four uninstrumented 1× runs use the exact existing candidate library
`fd572d9e18640414d3dd480ae6d2fad5b166cdbd9377ac65009b527e17d96a0f`.
Both interior cases pass the existing two-channel gross-content oracle. Near-B
captures contain a B-to-A wrap and continue to EOF, but their API-selected restart
point is not independent proof that this wrap was already submitted. Those
descriptive traces remain separate from the following diagnostic evidence.

A fresh isolated build copies all 961 candidate source inputs and changes only
`audio/out/ao_coreaudio.c`: read-only full-buffer/endpoint fingerprints, callback
timestamps and reset logging. It changes no sample, filter, transport or clock
calculation. Logging can affect scheduling; this diagnostic is not part of the
maintained functional patch series or an adopted runtime. All 39 upstream tests
pass. Its library SHA256 is
`2762bb8a0c1eb50510947a9bcdb238650ad4c6ae0fd5b0657f2a62989782ef46`.

Twelve cases cover 0.5×/1×/1.5×, interior/near-B and playing/already-paused clear.
The harness compiles the actual current Rust policy byte-for-byte, owns one native
handle on one thread and captures only that process. Independent PCM renders use
the same filter implementation; this is not independent WSOLA grain accounting.

| Check | Result and limit |
| --- | --- |
| State, capture timestamps, rate, disabled endpoints, EOF and normal shutdown | All 12 pass; held cases pause for at least 300 ms before and after clear. |
| Every nonempty callback before/after reset | All 12 match complete independent loop/exact-seek renders, including suffix EOF. No nonempty callback occurs between clear resets or during held clear. |
| Restart versus verified submitted endpoint | All 12 meet the retained half-source-sample bound. The 1.5× residual is approximately −0.5 sample; other rates differ only by floating-point residuals. Verified 1×/1.5× near-B cases already submitted a wrap. |
| Process-capture suffix correspondence | All 12 pass in both channels. This alone cannot establish a correct restart target. |
| Complete submitted-sequence correspondence | Eight pass in both channels. Four 1×/1.5× near-B cases retain failures from repeated-reference ambiguity: three lack a unique anchor; one has 67.51% coverage, below the unchanged 80% requirement. |
| Separate ordered boundary audit for those four cases | Pre-command single-loop anchors plus the verified prefix and native zero-frame count predict the independently matched suffix origin within 0.166 output samples, in both channels. This does not relabel the four combined-reference failures as passes. |

The diagnostic records no empty callback between old/new content in its playing
cases, and 29,184 zero frames in each held case. These are observed submissions,
not a seamless-output guarantee; the earlier interruption evidence remains valid.
The full-sequence matcher retains its 2,048-frame window, 256-frame hop, 0.97
correlation, 0.10 uniqueness margin, 80% coverage and two-frame displacement bounds.
The ordered audit rejects all 12 delete/repeat/extra-silence corruptions of 512
frames in both channels. Its independent submission matcher passes the unchanged
positive control and rejects four content/pause corruptions. Native hash checking
passes two controls and rejects seven byte/order/completeness corruptions.

Evidence is retained under `test-results/native-reliability-20261018/`:

- `clear-current-v1/`: exact-candidate captures, source/harness hashes, existing
  interior checks and descriptive near-B correspondence.
- `coreaudio-submission-v1/`: declared matrix, one-file diagnostic patch and
  961-file identity, source/build/test receipts, captures, reference PCM, all
  passing and failing analyses, corruption controls and cleanup receipts.

Recorded commands use Python 3.14.7, Meson 1.12.1, Ninja 1.13.2 and Rust 1.98.1.
Setup uses the existing candidate's Cocoa/Swift/libmpv/tests configuration with
VapourSynth and manual/media-player builds disabled, followed by
`meson compile -C BUILD -j 4 mpv:executable mpv:shared_library` and
`meson test -C BUILD --print-errorlogs -j 4`. The retained `setup.json`,
`harness-build.json` and `build-runtime-receipt.json` contain exact paths/commands.
The capture/analysis script snapshots are part of those local evidence directories;
their output files must not be overwritten. The recorded analysis sequence is:

```sh
python3 "$E/capture.py"
python3 "$E/render-current.py" baseline
python3 "$E/render-current.py" suffix
python3 "$E/analyze-native-current.py"
"$PY" "$E/analyze-state.py"
"$PY" "$E/analyze-suffix.py"
"$PY" "$E/analyze-submission-current.py" # exits 1: four retained ambiguities
"$PY" "$E/ordered-boundary.py"
"$PY" "$E/ordered-negative.py"
"$PY" "$E/controls.py"
```

Here `E` is the diagnostic evidence directory and `PY` is the existing bundled
Python with NumPy. Seven production transport-policy tests pass. The maintained
fingerprint/loop/clear analyzer suites pass 12/18/8 tests respectively; exact
commands/results are recorded in `analyzer-regressions.json`. Loaded-library receipts verify 45 non-system
files; system cache images are not individually hashed. All 50 owned capture and
reference-render processes across both stages exit normally and are absent.
The production policy, candidate library and functional patch series are unchanged.

At this audit the Linux VM had no logged-in user audio session: `/run/user/1000` and
its Pulse/PipeWire services were absent. That was an environment precondition, not
a reproduced clock failure or a pass. The historical 98 µs Pulse backstep remains
open. Next audio work must cover midstream rate changes, outside-range re-entry
and replacement, then current Linux/Windows output and full-editor behavior.

## Rate-change clear audit — 2026-10-18

Eight new diagnostic cases change 0.5×→1.5× or 1.5×→0.5× immediately before
production clear, at the loop interior or near B, while playing or already
paused. They use the same separately identified `2762bb8a…` tracing library and
current production Rust from the preceding audit. The harness now disables all
ten builtin scripts, matching the production adapter. All nonempty pre-clear
buffers match independent old-rate renders before deriving a submitted endpoint;
all post-clear buffers match independent new-rate exact-seek renders through EOF.
The endpoint stays within the unchanged half-source-sample bound. All eight
state/EOF, two-channel captured-suffix and complete submitted-sequence checks pass;
full-sequence coverage is 92.43–99.15%, with unchanged matcher thresholds.

This still reproduces a quiet interval. Three of four playing diagnostic cases
submit one empty 512-frame callback (10.6667ms). Held cases submit 29,184 zero
frames around their intentional pauses. Four additional playing controls use the
exact uninstrumented `fd572d9e…` candidate: state/EOF and new-rate suffixes pass,
but three contain 417 consecutive exactly-zero captured frames (8.6875ms), in
both channels. The capture and native callback counts describe different points
in the output path. The uninstrumented controls have no callback instrumentation
and do not independently establish the old submitted endpoint.

The quiet interval is unresolved. Source-order correspondence is not seamless
output or physical speaker evidence. No timing tolerance was relaxed, delay
added, clock clamped or functional runtime/transport change made. Both tested
rates use float output; sustained mixed-rate playback and transitions between
1× and non-unity output formats still need separate evidence.

Evidence under `test-results/native-reliability-20261018/`:

- `coreaudio-rate-change-v1/`: declared eight-case protocol, exact harness/source
  snapshots, captures, native traces, independent renders and all analyses.
- `coreaudio-rate-exact-v1/`: four separately identified uninstrumented controls.
- `inspect-rate-gaps.py`, `rate-gap-observations.json` and its log: descriptive
  zero-run measurements without changing the content verdicts.
- `finalize-rate-editor.py` and its log: all 961 original/diagnostic source inputs
  reverified, only the original tracing file differs, both library hashes match,
  45 non-system loaded backing files verified per variant, and all 36 owned
  player/recorder/reference-render PIDs exited normally and are absent. Each
  directory has a cleanup/runtime receipt and file-hash manifest.

The retained scripts run with the same Python/NumPy and Rust toolchains as the
preceding audit. For each new evidence directory, the sequence is `capture.py`,
`render-current.py suffix`, `analyze-state.py` and `analyze-suffix.py`; the
diagnostic additionally runs `analyze-native-current.py` and
`analyze-submission-current.py`. Existing baseline render identities are retained.
These scripts and their exact command receipts are local evidence, not a promise
that an archived output directory can be rerun in place.

## Editor audio re-entry and replacement — 2026-10-18

The maintained [Mac desktop audio lane](../tests/desktop/README.md#native-audio-re-entry-and-replacement-macos)
now exercises the actual WebView, Tauri IPC, media worker and selected runtime,
using deterministic four-second stereo PCM WAVs. `test-results/desktop-JiO4mU/`
passes all five process-only captures in both channels: initial looping,
past-B playback through EOF, re-entry, looping after a before-A seek, and
replacement audio through EOF. Replacement clears the old loop and refuses an
old-session seek. Exact paused seeks, original subtitle revision/text/clean state
and all source hashes remain correct. The before-A lead-in is observed through
status; its complete PCM is not captured.

The content oracle reuses the maintained fractional fingerprint and loop checks.
Uninterrupted suffixes require their requested beginning and EOF, sufficient
matched windows, no unexplained source deletion/repetition, and no detected
interior silence. Five new controls cover valid capture-edge silence, wrong start,
missing EOF, deleted/repeated content, wrong replacement/empty audio and inserted
silence. The complete `tests/native/check-controls.py` run passes 72 tests across
11 suites. ESLint, Prettier, Python compilation and `git diff --check` pass.
No production code changed; the earlier 507-test Rust result is not a fresh run.

The live receipt verifies 46 mapped backing images, including unchanged desktop
`2c431dab…` and exact candidate `fd572d9e…` from the WAV audit. All five recorder
exits and application shutdown are code0; the app and recorders are absent after
cleanup. Input/tool hashes, raw PCM/metadata, status rows, separate content verdict,
runtime identity and cleanup receipts are retained. The first successful run
`desktop-s2ovN7` and the initial ESLint failure remain recorded separately; the
final run includes strict interior-silence/PID checks and runtime identity.

The lane uses supplied file choices and typed actions. It does not establish
physical pointer/keyboard delivery, OS-sheet behavior, speaker presentation,
sample-exact acoustic timing or acceptance on other operating systems. It does
not close the separate rate-change interruption. Stage8 remains in progress.

## Current Linux output and portal checks — 2026-10-18

After the user logged into Ubuntu26.04 ARM64, PipeWire1.6.2 and its PulseAudio
compatibility server were available on the virtual HDA output. Root had only
613MiB available. These checks reused the existing candidate executable, wrote
fixtures/results to tmpfs and exported them to the host share; no install,
rebuild, system audio setting or unrelated file cleanup was performed. Root free
space remained approximately613MiB afterward.

The selected executable SHA256 is
`898a311a2f6eae6256d7a589ed2925f76eb93e448711f228d2329039068c9fd2`.
All961 source inputs match the prepared candidate. Separate live Pulse/PipeWire
receipts verify212 mapped backing images each, with no missing expected image.
The current commands run the maintained component probes with system Python:

```sh
python3 tests/native/audio-transport-regression.py --mpv "$PLAYER" --ao pulse --output-dir "$NEW_OUTPUT"
python3 tests/native/audio-transport-regression.py --mpv "$PLAYER" --ao pipewire --output-dir "$NEW_OUTPUT"
python3 tests/native/audio-loop-seek-regression.py --mpv "$PLAYER" --ao "$BACKEND" --output-dir "$NEW_OUTPUT"
python3 tests/native/audio-eof-regression.py --mpv "$PLAYER" --ao "$BACKEND" --paused-reset --output-dir "$NEW_OUTPUT"
python3 tests/native/audio-transport-regression.py --mpv "$PLAYER" --ao pulse --capture-pulse --output-dir "$NEW_OUTPUT"
```

The first two commands run three times each. `BACKEND` is separately `pulse` and
`pipewire`; each command creates its own result subdirectory. Results:

| Check | Pulse | Direct PipeWire |
| --- | --- | --- |
| Three loop/pause/resume/clear clock trials | All fail only the backward-clock assertion: maximum steps0.480,0.794,8.698ms | All three pass sampled clock checks |
| Past-B exact/approximate seek and before-A re-entry | All three pass | All three pass |
| Ordinary/paused-tail EOF, seek and replacement | All six pass | All six pass |
| One explicit single-stream monitor capture | Clock and coarse440→660→880Hz content pass | Not run in this matrix |

The tolerance remains one48kHz frame (20.833µs). Failing clock pairs have neither
seek nor EOF active. One passing monitored run does not erase the three unmonitored
failures: monitoring can alter scheduling. Tone order/duration checks do not
establish sample continuity, playhead-to-presentation accuracy or physical output.
Three direct-PipeWire passes do not resolve its historical intermittent failures.

A separate comparison keeps that tolerance and changes only the existing
per-process option `--pulse-latency-hacks=no`. Three default ordinary-playback
controls fail with0.130,0.270,0.884ms backsteps. Three ordinary and three loop/clear
trials with the option pass. The selected source uses libpulse interpolation for
this option while retaining `PA_STREAM_NOT_MONOTONIC`; no application clock clamp
was added. This narrows the investigation to a possible supported configuration,
not a validated production fix. Longer/rate/seek/replacement content and actual
editor checks remain required before adopting it.

The private-bus portal regression initially times out because its mock still
expects the old SRT-only filter. The application correctly requests all four
supported subtitle formats. The mock now validates those exact filters and
returns an explicit D-Bus error for malformed requests, avoiding an unanswered
method call. All six maintained success/cancel/disconnect/owner-replacement/
bus-loss/restart cases pass. Four malformed filter/title/modality/missing-field
controls receive the expected error in under1ms. Production dialog code is unchanged.

To avoid a large link with little free space, the VM reused the existing
`91e6ee1` application test executable
`19199a9c2607252d1c37b772dea6def157ddf94156f4ab2e4736ebd82720403d`.
Its prior build/workspace-test log identifies the executable; current dialog
source, Cargo manifest and lock hashes are rechecked. This is a focused rerun,
not a fresh whole-app build. The Linux desktop CI job now invokes the maintained
`/usr/bin/python3 -I tests/native/linux-dialogs/check.py` after its fresh workspace
build so future format changes exercise this otherwise-ignored test. Protocol
checks do not establish native parenting, real modality or forced parent death.

Exact commands, source/tool identities, raw successes/failures,212-image receipts
and156 verified exported artifacts (13,309,893bytes) are under host staging
`Downloads/VisualSubForge-native-audio-k6f2faxo/`; the stable local index and logs
are `test-results/native-reliability-20261018/linux-*.json`/`linux-*.log`.
`run-linux-audio.py`, `run-linux-followup.py`, the retained corrected v2 script
and `portal-negative.py` record the bounded reproduction. All25 first-matrix
player identities are absent; no player remains for either owned tmpfs workload.
Captured sink-input/source-output cleanup is recorded. Original portal failure
and default-Pulse failures remain evidence, not passing results. Physical Linux,
Windows output, native gesture/lifetime and distribution gates remain open.

## Linux Pulse content extension — 2026-10-18

The same executable and two deterministic four-second stereo48kHz fixtures now
have process-specific Pulse captures. With `--pulse-latency-hacks=no`, ordinary
playback, the1.013–1.336s loop and a complete replacement-source suffix pass the
unchanged fractional fingerprint checks in both channels. The ordinary default
control fails: its first matched source frame is2048, so the captured beginning
is incomplete. Monitor startup and cold/warm ordering remain confounded; this
does not establish lost physical output or a causal option fix. The replacement
check covers the new source, not the old/new join or transition latency. Capture
timestamps identify monitor receipt, not presentation.

All three outside-range seek/re-entry cases pass with the option. The first EOF
attempt omitted `audio-eof-observer.lua` from local staging; the retained logs
explicitly report that missing script. After copying that unchanged observer,
all six ordinary/pause/seek/reload EOF cases pass. These are setup failures and a
corrected rerun, not a repaired player. Sustained mixed-rate checks and actual
Linux editor integration remain pending; the staging protocol's proposed mixed-
rate check was not executed. Production runtime/configuration is unchanged.

The reproduction is `run-linux-pulse-content.py`, followed by
`analyze-linux-pulse-content.py HOST_STAGING` with bundled Python/NumPy. The index,
analysis and corrected EOF logs are under
`test-results/native-reliability-20261018/linux-pulse-*`; source/captures, commands,
original failures and `content-analysis.json` are in host staging
`Downloads/VisualSubForge-native-audio-k6f2faxo/pulse-content-v1/`.
The final receipt verifies77 exported artifacts (10,489,789bytes), all four capture
player PIDs absent and no owned player/recorder remaining. Root retains
642,576,384bytes free; temporary guest outputs remain in tmpfs. No Linux install
or rebuild was performed.

## Native Mac pointer and keyboard check — 2026-10-18

The identified2c431dab... app with fd572d9e... libmpv opened an isolated three-cue
SRT and four-second WAV. Actual native input dragged the first end handle from
1.000s to1.221s and released below the waveform. One Undo restored1.000s and the
saved state with Undo disabled; Redo restored1.221s. Native Save As produced an
exact separate-file result. Focusing that boundary and pressing Right moved it
10ms; one Undo restored the saved result. Command-Q exited0, its PID is absent,
and source subtitle/media hashes remain unchanged.

The receipt is `test-results/native-reliability-20261018/mac-native-gesture-v1/verified-result.json`;
the accessibility/screenshot observations are retained in the conversation.
File selections were supplied to the isolated test app. This establishes native
pointer release outside the canvas and keyboard/history/save behavior; it does
not establish held-pointer Escape, focus-loss cancellation, native chooser
parenting or physical-platform acceptance.

## Hosted checks checkpoint — 2026-10-18

Commit71cf014 passes all six jobs in
[Native build contracts39141646095](https://github.com/golgeek/visualsubforge/actions/runs/39141646095),
including three pinned mpv build/upstream-test variants and three platform
adapter compilation jobs. Compilation is not runtime acceptance.
[Checks39141646080](https://github.com/golgeek/visualsubforge/actions/runs/39141646080)
has retained failures: Windows x64 expects a Protocol error for oversized/deep
plugin results, and WebKit reports an incorrect second statistics page count.
These did not reproduce in the Windows ARM64 direct worker probe or ten local
WebKit statistics runs. Eleven local worker tests pass. Assertions now report
the actual rejection error and rendered count without changing their criteria;
hosted diagnosis is pending. Original logs and browser trace are retained under
`test-results/native-reliability-20261018/ci-*`.
