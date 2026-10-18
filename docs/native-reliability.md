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

The Linux VM currently has no logged-in user audio session: `/run/user/1000` and
its Pulse/PipeWire services are absent. That is an environment precondition, not
a reproduced clock failure or a pass. The historical 98 µs Pulse backstep remains
open. Next audio work must cover midstream rate changes, outside-range re-entry
and replacement, then current Linux/Windows output and full-editor behavior.
