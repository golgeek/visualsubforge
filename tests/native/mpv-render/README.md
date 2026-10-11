# Upstream mpv rendering corrections

[series.json](series.json) pins upstream `master` commit
`c1529642089bfebfc928a1c1664638a7a5d219ba`, which already contains the
[f74adc4 fix](https://github.com/mpv-player/mpv/commit/f74adc4243bd9e1a8ee96d187c939e6232510650).
The original backport is retained as provenance, outside the applied patch list.
This prevents creation of internal swapchain fences when the embedder owns
presentation. It changes no VSF API, timing policy or renderer ownership.
Modified upstream source retains its LGPL-2.1-or-later license; see the adjacent
[audio experiment's upstream license copy](../mpv-clock/LICENSE.LGPL).

The preparation script verifies that this correction is in the pinned base. Its default
also applies the nine experimental audio patches and the separate
[CoreAudio cleanup series](../mpv-coreaudio-cleanup/README.md); `--render-only`
omits both audio-related series and produces unmodified upstream source:

```sh
python3 scripts/prepare-mpv-clock.py --source /tmp/mpv-upstream --output /tmp/vsf-mpv-render --render-only
```

The script verifies both manifest and patch hashes, copies committed source into
a new directory and records each applied series in `manifest.json`. It does not
build, install or modify the source clone. The historical script name remains
for existing audio-investigation commands. Existing prepared builds are not
updated; prepare and build a new directory. Do not infer this correction from
`vsf-audio-clock-version`: that marker describes the separate audio experiment.

On Linux with development dependencies available:

```sh
meson setup /tmp/vsf-mpv-render/build /tmp/vsf-mpv-render/source -Dlibmpv=true -Dcplayer=false -Dtests=false -Dmanpage-build=disabled
meson compile -C /tmp/vsf-mpv-render/build -j 2
cc -Wall -Wextra -Werror tests/native/mpv-render-fence-probe.c -o /tmp/vsf-render-fences $(pkg-config --cflags --libs epoxy mpv)
LD_LIBRARY_PATH=/tmp/vsf-mpv-render/build timeout 45 /tmp/vsf-render-fences
```

The probe requires an accessible EGL display (`DISPLAY`/`XAUTHORITY` for the
recorded XWayland test). It counts Linux `sync_file` descriptors every 100 calls,
allows 16 above baseline and checks 2,400 empty render calls. Exit 0 passes;
1 detects accumulation; 2 indicates setup/driver failure; timeout is a failure.
A stock 0.41.0 negative control fails after 100 calls on the supplied virgl VM.
Drivers need not represent GL fences as these descriptors, so a pass on another
driver is not proof of all GPU resource lifetimes. This is not video playback,
UI responsiveness, hardware-decoding, audio timing or macOS validation.

Task [003c](../../../docs/tasks/003c-desktop-composition.md) records bounded
before/after evidence, including independent Mac CGL and ANGLE/Metal fence
controls, Mac editor command smokes and a two-minute 4K playback/lifecycle
soak. Sustained Linux editor playback and corrected-library interaction remain
separate validation requirements.
