# CoreAudio initialization cleanup experiment

[series.json](series.json) pins one local ownership follow-up against upstream
`master` commit `c1529642089bfebfc928a1c1664638a7a5d219ba`. The two upstream
corrections below are already in that base; their original patches are retained
for provenance and are no longer applied. The default
[`prepare-mpv-clock.py`](../../../scripts/prepare-mpv-clock.py) recipe applies
this series after the nine audio-clock patches; the render correction is upstream. Its
`--render-only` mode excludes both audio-related series. Existing prepared
libraries remain unchanged; prepare a fresh directory and build privately.

1. Upstream `af067b5` calls outer cleanup when CoreAudio initialization fails.
2. Upstream `c5d391a` registers device listeners only after AudioUnit setup succeeds.
3. The local follow-up clears `p->audio_unit` after the inner error path disposes
   it. Outer cleanup otherwise tries to stop/uninitialize/dispose the released
   handle again. This one-line correction is **not upstream accepted**.

The original inner function owns partial AudioUnit construction; outer cleanup
owns the remaining output resources. Clearing the released handle makes their
ownership explicit. Successful initialization transfers the live handle to
outer cleanup. No device policy, clock formula or application API changes.
The modified upstream code retains its LGPL-2.1-or-later license; see
[the existing upstream license copy](../mpv-clock/LICENSE.LGPL).

## Regression

With a built private library, mpv development headers and an existing audio WAV:

```sh
python3 tests/native/coreaudio-init-regression.py \
  --library-dir /tmp/vsf-mpv-clock-prepared/build \
  --include-dir /opt/homebrew/include \
  --media tests/fixtures/generated/loop-content-v1/1ch.wav \
  --output /tmp/vsf-coreaudio-init-check
```

The output directory must be new. The Python runner compiles two small C helpers
with warnings denied. A process-local DYLD interposer forwards native calls,
except for the requested initialization or input-format failure. No interposer
is linked into VSF or mpv. Each failure case opens/destroys ten cores; a control
opens two successfully, paused, with mono CoreAudio explicitly selected and no
fallback. The tested library also needs the separate channel-layout correction
from the audio experiment for that healthy mono control.

Exit 0 requires all intended fault points, expected mpv errors, selected-library
identity, balanced unit/listener counts, no rejected disposal calls and normal
exit. Exit 1 rejects behavioral/resource/coverage failures; setup errors also
exit nonzero. Child runs have a 65-second watchdog. Counts cover successful
native API operations; listener counting is limited to callbacks originating in
libmpv. Do not infer acoustic timing, ordinary failure frequency, every setup
branch, hotplug correctness or whole-process memory budgets.

[Task 005](../../../docs/tasks/005-audio-loop-timing.md#coreaudio-cleanup-ownership-and-maintained-regression--2026-10-03)
records the three-library comparison: the earlier candidate leaks listeners;
upstream fixes remove that leak but issue duplicate disposal calls; the local
handle correction passes both fault cases and the healthy control. This remains
an opt-in dependency experiment, not acceptance of a release fork or resolution
of short-loop continuity.
