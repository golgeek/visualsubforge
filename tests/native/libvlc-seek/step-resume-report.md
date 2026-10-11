# Draft: libVLC 4 skips buffered video when resuming after frame stepping

Status: local investigation report; not submitted upstream. This reproduces on
the pristine core, independently of VSF's experimental paused-seek patch.

## Environment and reproduction

- VLC commit `b5eb753828de2bbec9a80bfee829b5d124e99043`.
- Locally built unmodified ARM64 core/public libraries; matching official Mac
  SDK plugins, with loader paths checked. Build provenance and commands are in
  [README.md](README.md) and the [feasibility record](../../../docs/libvlc-feasibility.md).
- macOS 27, Apple M3 Max. Software H.264 decode (`--codec=ffmpeg,none`), dummy
  video output, CoreAudio; `--no-audio` also reproduces.
- Original synthetic 12-second MP4: 640×360, 30 fps, numbered binary frame
  markers, AAC. Generate with [seek-frame-fixture.py](../../../scripts/seek-frame-fixture.py).

Compile [libvlc-step-resume-probe.c](../libvlc-step-resume-probe.c) using the
README's C11 strict flags, SDK headers and rpaths to the selected pristine core.
Run in a fresh existing output directory:

```sh
VLC_PLUGIN_PATH="$VSF_VLC_SDK/lib/vlc/plugins" DYLD_PRINT_LIBRARIES=1 \
  "$VSF_STEP_PROBE" "$VSF_MARKER_VIDEO" "$VSF_STEP_CASE" cycle \
  > "$VSF_STEP_CASE/stdout.log" 2> "$VSF_STEP_CASE/stderr.log"
python3 tests/native/check-seek-frame-id.py --fixture "$VSF_MARKER_VIDEO" \
  --case "$VSF_STEP_CASE" --output "$VSF_STEP_CASE/identity.json" --sequence cycle
```

The probe plays normally to approximately 1.5 seconds, pauses, requests next
then previous frame, resumes for 200 ms, then pauses again. It never seeks and
loads no subtitles. Each step waits for a successful status callback and a new
paused time point, then takes a snapshot. The separate image checker establishes
frame identity; callback completion and reported time alone are insufficient.
Control/API calls use one owner thread; callbacks publish observations only.
Bound the entire process with an external timeout in addition to internal waits.

## Expected and observed

Expected: next and previous return to the original image; resume advances video
by approximately the elapsed playback duration.

Observed with default file caching:

- Snapshot IDs: 47 → 48 → 47 → 88. Resume advances 41 frames during 228,846 µs.
- Reported time: 1,566,667 → 2,763,751 µs.
- The `plain` control (without steps) advances from frame 47 to 53 during
  228,609 µs.
- Disabling audio retains a large skip: 46 → 47 → 46 → 83 during 228,165 µs.
- Setting `--file-caching=100` reduces the discrepancy. This passes the test's
  deliberately generous 250 ms excess-progress bound, but does not establish
  frame-accurate resume.

The exact initial paused frame can vary with scheduling. The checker derives
the expected next/previous IDs from the initial image rather than assuming 47.
Snapshots establish engine output, not display or speaker presentation.

## Reduced trace

A separate logging-only build reproduces 47 → 48 → 47 → 88. Immediately before
resume, the retained picture is 1,566,667 µs while the last demux PCR is
2,752,001 µs. `EsOutResumeFromNextFrame` retains video, flushes other streams and
resets buffering/clock state. Buffering then completes with:

```text
stream_start = 2,766,667 µs
stream_duration = required_buffer = 1,000,000 µs
new system origin = 612116576539 µs
first retained render: PTS 1,566,667 µs
render wall time = 612117576597 µs
converted system time = 612116456539 µs
```

The retained frame is scheduled about 1.12 seconds in the past. Subsequent queued
frames are discarded as late. With 100 ms caching, the retained PTS is 1,533,334
µs, the new stream start is 1,792,001 µs, and the corresponding lateness is about
157 ms. This identifies a mismatch between retained video and the clock rebuilt
from demux read-ahead; it does not establish the correct general fix.

Do not infer that simply preserving a clock or disabling late-frame dropping is
safe: frame stepping can discard non-video packets, and previous-frame decoding
can seek the demuxer. Restoring a synchronized audio/video position needs to
account for those states. No correction is included in this report.

The retained local evidence is in `libvlc4-completed-step-v1` and
`libvlc4-resume-trace-v1` under `tests/fixtures/generated/`. Windows/Linux and
other media have not been tested for this reduced reproducer.
