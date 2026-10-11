# libVLC 4 paused-seek experiment

Status: **experimental dependency patch**, not an application dependency or accepted architecture decision. Plain paused seeking passes the bounded tests, but stepping followed by resume still exposes serious transport failures. The [feasibility record](../../../docs/libvlc-feasibility.md#native-output-validation-and-resume-failures) contains the current validation limits; earlier sections retain tracing and rejected alternatives.

`paused-seek.patch` applies to VLC commit `b5eb753828de2bbec9a80bfee829b5d124e99043`, paired with the official Mac ARM64 SDK recorded in that document. Source archive SHA-256: `6c12f97747e19ab0e9dbfdcd83653a25685a115e7c1111c8361d9daf440b5c11`. The modified upstream files, `src/input/es_out.c` and `src/video_output/video_output.c`, carry LGPL-2.1-or-later notices. Their existing ownership and synchronization remain in place; this patch adds no thread, callback, resource or application-side timestamp adjustment.

The four changes address separate observed mechanisms:

- Use the frozen pause date for input-clock updates during user pause, as already done during rebuffering. Wall time spent paused must not trigger late-PCR resets while supplying seek data.
- Prepare a new paused picture without dropping it as late against advancing wall time.
- Present that new picture immediately and publish its paused time point. Ordinary redisplay retains its existing behavior; it must not introduce extra pause/timer notifications.
- Treat a positive-duration video packet ending exactly at the requested time as preroll. This implements the tested half-open frame interval without changing zero-duration packets or audio comparisons.

The last rule is verified on the synthetic 30 fps fixture only. General VFR, reordered video and demuxer duration semantics remain open. Do not infer broad format support from this patch's small size.

## Reproduce on the tested Mac

Use a **fresh extracted source directory** and new build directory. Set these absolute paths before running commands:

```sh
VSF_REPO="$PWD"
VSF_VLC_SOURCE=/absolute/path/to/fresh/vlc-source
VSF_VLC_BUILD=/absolute/path/to/new/vlc-build
VSF_VLC_SDK="$VSF_REPO/tests/fixtures/generated/libvlc4-mac-v1/sdk"
VSF_SEEK_CASE=/absolute/path/to/new/seek-result
```

The tested build uses Meson 1.12.1 and Ninja 1.13.2.git.kitware.jobserver-pipe-1 from `/tmp/vsf-mpv-build-tools/bin`. The pkg-config paths below are specific to the tested ARM64 Homebrew/macOS 27 host, and avoid an unrelated x86-only `/usr/local` library. These changes are shell-local.

```sh
export PATH="/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH"
unset PKG_CONFIG_PATH
export PKG_CONFIG_LIBDIR=/opt/homebrew/lib/pkgconfig:/opt/homebrew/share/pkgconfig:/opt/homebrew/Library/Homebrew/os/mac/pkgconfig/27
patch -p1 -d "$VSF_VLC_SOURCE" -i "$VSF_REPO/tests/native/libvlc-seek/paused-seek.patch"
meson setup "$VSF_VLC_BUILD" "$VSF_VLC_SOURCE" \
  -Dauto_features=disabled -Dvlc=false -Dtests=enabled \
  -Dextra_checks=true -Dbuildtype=debugoptimized
meson compile -C "$VSF_VLC_BUILD" -j6 vlccore vlc
```

This builds only the core/public libraries. The real-media probe deliberately loads the matching SDK plugins; it is not a standalone packaging recipe. Compile and run the public-API regression:

```sh
mkdir "$VSF_SEEK_CASE"
clang -std=c11 -Wall -Wextra -Werror -pthread \
  -I"$VSF_VLC_SDK/include" tests/native/libvlc-paused-seek-probe.c \
  -L"$VSF_VLC_BUILD/lib" -lvlc \
  -Wl,-rpath,"$VSF_VLC_BUILD/lib" -Wl,-rpath,"$VSF_VLC_BUILD/src" \
  -Wl,-rpath,"$VSF_VLC_SDK/lib" -o "$VSF_SEEK_CASE/probe"
VLC_PLUGIN_PATH="$VSF_VLC_SDK/lib/vlc/plugins" DYLD_PRINT_LIBRARIES=1 \
  "$VSF_SEEK_CASE/probe" \
  "$VSF_REPO/tests/fixtures/generated/libvlc4-frame-id-v1/frame-id.mp4" \
  plain "$VSF_SEEK_CASE" > "$VSF_SEEK_CASE/stdout.log" 2> "$VSF_SEEK_CASE/stderr.log"
python3 tests/native/check-seek-frame-id.py \
  --fixture tests/fixtures/generated/libvlc4-frame-id-v1/frame-id.mp4 \
  --case "$VSF_SEEK_CASE" --output "$VSF_SEEK_CASE/identity.json"
```

See the [fixture recipe](../../fixtures/README.md#binary-frame-identity-fixture--libvlc-seek-investigation) if the synthetic MP4 is absent. Require both probe exit 0 and independent image-check exit 0; the probe alone has only a coarse position threshold. Inspect the loader log to confirm the intended core. `resume-after-seek` additionally requires playback progress and a return to pause after each snapshot, rejecting reported progress more than 250 ms ahead of elapsed wall time. This generous failure threshold is not an editing-precision budget or proof of audio/device alignment. Use a fresh result directory for each run.

Build the runtime modules **as well as the upstream test executables** before using `--no-rebuild`:

```sh
meson compile -C "$VSF_VLC_BUILD" -j4 \
  demux_mock_plugin araw_plugin rawvideo_plugin subsdec_plugin \
  vdummy_plugin wdummy_plugin console_logger_plugin \
  test_src_player_pause test_src_player_seeks test_src_player_next_prev test_src_player_timers
VLC_PLUGIN_PATH="$VSF_VLC_BUILD/modules" \
  meson test --no-rebuild -C "$VSF_VLC_BUILD" --print-errorlogs -j1 \
  test_src_player_pause test_src_player_seeks test_src_player_next_prev test_src_player_timers
```

The tested candidate passes all four groups, plain-seek exact images at three boundaries and ±1 µs, and three ordinary seek/resume/pause checks with the elapsed-time guard. Native Mac output also passes the three exact-boundary seeks using either VideoToolbox or software decoding. However, adding next/previous-frame operations before resuming exposes clock jumps, and the visible native sequence also reached an unexpected stopped state. Paused subtitle updates and Japanese glyphs still fail. This patch is consequently **not ready for application integration**. Rapid replacement, broader media, Windows/Linux and task-005 short-loop timing remain unverified or unresolved. No upstream report or patch submission has been sent.

## Isolate stepping from paused seeking

`../libvlc-step-resume-probe.c` plays normally to about 1.5 s before pausing. Compile it with the same flags and selected build paths above. Its arguments are `VIDEO EXISTING_OUTPUT_DIRECTORY plain|cycle|restore|queued`. Plain and cycle never seek. The cycle waits for each successful next/previous status callback and a new paused time point, then independently saves the resulting picture. Callbacks only publish observations; the control thread owns library calls and releases the player before its static callback storage can expire. A supervisor should bound the entire process as well as the probe's internal waits.

Run the image checker with `--sequence plain` or `--sequence cycle`, alongside its existing `--fixture`, `--case` and `--output` arguments. It checks the source's 360 markers and PTS, exact one-frame next/previous images, completed observations, and resumed image progression against elapsed time plus 250 ms. This is a gross-jump detector, not a frame-accurate resume or audio-presentation contract. Missing images or completion records fail. Snapshot images are engine output, not screen presentation.

The separate `restore` mode re-seeks to the previous-frame timestamp before
resuming. It waits for seek-end, allows 100 ms settling, then saves the image;
seek-end is not treated as presentation proof. `--sequence restore` additionally
requires the restored image to match the previous frame and the restored clock
to identify that source frame within 2 µs. This precision check is specific to
the synthetic CFR fixture. Wrong/missing images, missing restoration results,
and plausible-looking but incorrect clock values are negative controls.

The `queued` mode requests a precise seek to the completed previous-frame
timestamp and resumes immediately, without a settling delay. Use
`--sequence queued` to require its accepted target and seek completion alongside
the step/image progression checks. This avoids the gross video jump in bounded
controls, but capture exposes audio before the requested target, also reproduced
with ordinary precise seeking on the pristine core. It is a diagnostic policy,
not an accepted workaround. See the
[captured audio evidence](../../../docs/libvlc-feasibility.md#immediate-restore-policy-and-captured-audio-seek-boundaries).

The [completed-step comparison](../../../docs/libvlc-feasibility.md#completed-step-baseline-and-cache-isolation) reproduces an actual video skip in both pristine and patched cores, without seeking or subtitles. Disabling audio still reproduces it. A 100 ms file-cache control passes the generous bound but still advances nine frames in about 230 ms; it is not an adopted workaround. The failure therefore predates this patch, while patch safety across broader inputs remains unproven.

## Additional isolated corrections

Two further experimental corrections and a test patch are retained separately from `paused-seek.patch`:

- `cancel-stale-frame.patch` checks flush/abort after the previous-frame wait.
  The wait releases the existing decoder FIFO lock, so a retained picture can
  become invalid while waiting. The same owner releases that picture on
  cancellation; no new lock, thread or resource is introduced.
- `preserve-paused-origin.patch` preserves the main clock's pause date when
  resetting during pause. The existing clock mutex protects this state. This
  works with the frozen pause-date references in the paused-seek experiment;
  it is not a general recommendation for unrelated clock users.
- `paused-reset-regression.patch` adds a deterministic upstream clock-test
  scenario: rebuild a reference during pause, establish its mapping, resume
  after 500 ms, and require the mapping to shift by 500 ms. The original clock
  fails this scenario; the origin-preservation change passes.

For the tested combined experiment, apply these patches in the listed order
after `paused-seek.patch` to a fresh pinned source checkout. The test-only patch
adds the `paused_reset` scenario to `test_src_clock_clock`; run that executable
with the scenario name as its argument. The [detailed evidence](../../../docs/libvlc-feasibility.md#stale-picture-cancellation-and-paused-clock-origin)
records build commands, source reconstruction and the remaining failures.

The combined experiment restores the right image and eliminates the gross skip
in one explicit re-seek/resume control. Ordinary resume after stepping still
skips; reported time after same-position restoration also disagrees with the
image. It is **not ready for integration**. All four player groups pass. The
full clock suite fails its `drift_72` assertion with both original and modified
clock code; targeted existing pause/context scenarios and the clock-start group
pass. Do not report the whole clock suite as green.

`rehydrate-timer.patch` is a later isolated timer correction. After a seek
invalidates a timer reference, accept a new output point even if its timestamp
equals the last reported timestamp. Duplicate suppression must not leave that
reference invalid. The existing timer mutex and callback ownership are retained.
Apply it after the code patches above for that experiment. The maintained
restore regression fails without it and passes with it, including independent
image/clock checks; all four player groups pass. However, audio-enabled resume
still reports a stale timestamp, and a video-only control stops advancing and
reports time moving backward. Their cause and relationship to this change remain
unresolved. See the [timer evidence](../../../docs/libvlc-feasibility.md#same-position-timer-restoration-and-remaining-resume-failures).

Two subsequent experiments address distinct pause-reference problems:

- `shift-pause-reference.patch` shifts valid player-timer references by the
  paused duration on resume and notifies ordinary time watchers of the new
  mapping. It leaves invalid/frozen references unavailable for interpolation.
  `timer-pause-probe.c` checks getter and watcher behavior at 0.5×, 1× and 2×,
  including a repeated resume event and a second pause without new timestamps.
  It compiles the pinned timer implementation into a diagnostic executable;
  it is not an application API or a replacement timer implementation.
- `freeze-monotonic-origin.patch` uses the frozen pause date when creating a
  monotonic clock reference during pause. Otherwise resume shifts a reference
  that already includes some paused wall time. `paused-reset-late-regression.patch`
  is an alternative upstream clock-test patch, applied to a fresh test file
  instead of `paused-reset-regression.patch`; it delays initial conversion by
  200 ms while paused and requires the reference to remain frozen.

Apply the code patches after `rehydrate-timer.patch` in that order for the
combined experiment. Both deterministic regressions fail without their
respective correction and pass with it. Real-media controls improve ordinary
audio pause/resume reporting and restore video-only progression. The audio clock
after explicit restoration still stalls, and unassisted frame-step resume remains
unresolved. These are bounded software/dummy-output results, not task-005 or
cross-platform acceptance. See [validation and remaining limits](../../../docs/libvlc-feasibility.md#pause-resume-reference-shifting-and-frozen-monotonic-origins).

`first-output-mapping.patch` is the next isolated correction, applied after
`freeze-monotonic-origin.patch`. It retains first-point interpolation suppression
at startup and for input/SMPTE updates, but preserves the supplied output mapping
when an established best timer recovers from discontinuity. Explicitly frozen
dates remain frozen. A broader variant broke the upstream startup contract and
was rejected; it is retained only in generated evidence.

`timer-first-output-probe.c` checks the private helper with synthetic timestamps.
Compile it beside the matching `timer-under-test.c` using the core compiler
flags, player include path and libraries, as for the pause probe. Define
`FIRST_OUTPUT_MAPPING` for the candidate signature; omit it for the baseline.
It explicitly supplies the accepted-point state normally maintained by the
caller. Baseline returns 1 and candidate returns 0; the real player timer group
also passes with the narrower patch. Exact commands are in
`tests/fixtures/generated/libvlc4-first-output-unit-v2/`.

This fixes the observed frozen audio report after corrective seek/resume, not
the full transport contract. The settled image still leads the reported audio
clock in a final control, and ordinary step/resume still skips approximately
41–42 frames in 230 ms. Four player groups and clock-start pass; the previously
recorded full-clock-suite failure remains open. See the
[first-output evidence](../../../docs/libvlc-feasibility.md#first-output-mapping-after-discontinuity).
The application and installed dependencies remain unchanged.


`audio-preroll.patch` is the eighth code patch, applied after
`first-output-mapping.patch`. It propagates the exact seek target through
reference-counted compressed-frame ancillary data, reads it before packetization,
and trims decoded linear PCM to the nearest sample (ties forward). The existing
FIFO lock protects the decoder target; both flush paths reset it. Payload-view
changes preserve the frame allocation and release callback. Unsupported
passthrough retains existing behavior. This is start trimming, not loop-end
clipping or a complete transport correction.

`audio-preroll-unit.c` exercises sample identity, stereo stride, whole-block
rejection, rounding, malformed buffers and timestamp overflow. Compile with the
matching core's private include/compiler flags and link its core library. Exact
normal and ASan/UBSan commands/results are retained in
`tests/fixtures/generated/libvlc4-audio-preroll-unit-v2/`; this tests helper/frame
ownership, not decoder concurrency or allocation-failure handling. Four player
groups, clock-start and nine video-boundary image checks also pass. The previous
full-clock-suite failure remains open.

Real decoded PCM-prefix tracing confirms the requested 2.3 s start on the lossless
fixture, but process-output onset/clock precision and loop ends remain unaccepted.
The final loop capture exceeds B by about 27 ms. See the
[audio-preroll evidence](../../../docs/libvlc-feasibility.md#exact-audio-preroll-propagation-and-decoded-pcm-trimming)
for ownership, rejected upward-rounding variant, independent waveform checks,
reconstruction, and remaining limits. Do not integrate this series based on
helper tests or process exit status alone.


A subsequent decoded-endpoint clipping control is **rejected**, remains only in
generated evidence, and is not a ninth patch to apply. It bounds captured range
output but discards source content needed when clearing the loop, causing a
large forward jump absent from the two earlier controls. Its passing helper and
upstream A–B tests do not close that failure. See the
[endpoint/clear comparison](../../../docs/libvlc-feasibility.md#decoded-endpoint-clipping-and-clear-regression)
for the retained patch, captures and next ownership/pacing questions.


The subsequent retained-tail control also fails real output coverage and remains
only in generated evidence. The [maintenance checkpoint](../../../docs/libvlc-feasibility.md#maintenance-and-migration-checkpoint)
stops expansion of VLC core patches as the default next action while preserving
the existing editor and all timing/rendering requirements. No backend migration
is justified by the current evidence.
