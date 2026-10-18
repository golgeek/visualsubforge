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
