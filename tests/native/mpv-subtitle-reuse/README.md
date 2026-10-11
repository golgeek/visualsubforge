# Retained subtitle renderer experiment

Status: diagnostic only. Not a product API, an installed dependency, or part of the task 005 audio-clock patch series.

`manifest.json` pins the official mpv 0.41.0 commit and this patch's SHA-256. Apply it to a new checkout of that commit; do not modify the audio-clock investigation checkout. The modified upstream files retain their LGPL-2.1-or-later notices; see [the retained license](../mpv-clock/LICENSE.LGPL).

The local command `vsf-replace-ass <selected-track-id> <complete-ass-text>` parses a replacement track under the existing subtitle decoder mutex. That mutex also protects packet decoding and bitmap generation. Parsing occurs before the track swap. The ASS library and renderer remain owned by the decoder, and ordinary decoder teardown frees them. Video-frame PTS still flows through mpv's existing subtitle pipeline; VSF does not supply a polled clock.

The experiment requires a selected, preloaded external ASS track, an enabled renderer, fixed subtitle options, and ordinary nonanimated preview text. It rejects converted SRT decoders, clear-on-seek, stripped/disabled ASS output, a wrong track ID, unavailable playback and payloads over 4 MiB. Those guards bound this diagnostic; they are not a complete document-validation contract.

Known gaps before any product adoption:

- Option-driven decoder rebuilds and subtitle filters can restore the original demux data. Accepted preview content needs an explicit retained source/replay contract.
- Track IDs can be reused across media loads. They are not session/revision guards.
- The original packet cache still describes the loaded track; audio-only redraw scheduling, animated subtitles and filter behavior are not established.
- Parsing holds the decoder lock. Large-document latency, memory limits, cancellation and malformed/partially accepted ASS need testing and policy.
- Windows/Linux dependency builds, sanitizers, track-switch races and integration with the audio-clock patch series are unverified.

Mac build used:

```sh
git clone --no-hardlinks --no-checkout /tmp/vsf-mpv-clock-source /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/source
git -C /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/source checkout --detach 41f6a645068483470267271e1d09966ca3b9f413
git -C /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/source apply "$PWD/tests/native/mpv-subtitle-reuse/0001-preview-replacement.patch"
PATH=/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH meson setup /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/build /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/source -Dlibmpv=true -Dcocoa=enabled -Dswift-build=enabled -Dmacos-media-player=disabled -Dtests=true
PATH=/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH meson compile -C /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/build -j 6 mpv:executable mpv:shared_library
PATH=/tmp/vsf-mpv-build-tools/bin:/opt/homebrew/bin:$PATH meson test -C /tmp/vsf-mpv-subtitle-reuse-dxllu4qe/build --print-errorlogs -j 6
```

These paths identify the completed experiment. Use a new output checkout when reproducing. The clone uses only committed upstream content; it does not copy the source checkout's uncommitted audio experiments. The recorded build was prepared by applying the equivalent source edits before exporting this patch; reverse-apply verification and comparison with a fresh patched upstream tree check reproducibility.

The ANGLE probe accepts the optional `VSF_ANGLE_IPC` socket path so a local diagnostic driver can send commands and request screenshot files. Leave it unset for ordinary probe use. Only subtitle text, scalar status and commands cross this socket; the diagnostic screenshot command writes files inside mpv. Build linkage, exact commands, 32 passing upstream tests, desktop observations, pixel comparisons and limitations are recorded in [task 003c](../../../docs/tasks/003c-desktop-composition.md).
