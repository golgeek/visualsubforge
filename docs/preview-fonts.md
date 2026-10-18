# Subtitle preview fonts

Task 018 is complete within the Mac runtime scope below. Accepted subtitle text,
style declarations, save bytes and history are unchanged by font selection.

## Rendering and notices

libass resolves declared fonts, font attachments and platform fonts. On the
tested Mac, generic CJK fallback reaches a PingFangUI font that FreeType cannot
open. The existing mpv `subfont.ttf` last-resort mechanism now points to an
installed Hiragino Sans GB font discovered through CoreText. Named/embedded fonts
retain precedence. No system font is installed, copied or redistributed. This
does not promise coverage of every Unicode character or identical typography
across platforms. If the fallback is unavailable, playback retains its ordinary
font behavior and reports the limitation.

The editor's **Subtitle font notices** disclosure reports unavailable fonts,
substitutions and missing glyphs observed during the media session. A fallback
can succeed after an earlier font fails. Notices include earlier displayed cues;
they are not assigned to the currently selected cue. Close/replace media clears
the session. At most eight distinct messages of 511 bytes plus one omission
notice are retained. Absence of notices is not proof that all cues were rendered.
Notices cannot change document state or prevent editing/saving.

## Ownership

The native control owner creates an atomic mode 0700 temporary directory containing
only a symlink to the installed OS font. It selects that directory with mpv's
explicit config-dir. Config lookup must be enabled for this resource lookup, but
the pinned mpv path policy excludes other config roots; the private directory has
no configuration or scripts. Built-in and external scripts remain disabled.
The directory remains alive through rendering/core teardown and is removed on
normal destruction and initialization failure. Abrupt process termination can
leave this small private directory for the OS temporary-directory cleanup.

No UI-thread IO or new worker is introduced. The existing control thread copies
bounded native diagnostics into typed media status. Logs have no document revision
identity, so their lifetime is explicitly the media session. They never become
document findings. The default non-media build returns an empty notice list.

ASS/SSA `[Fonts]` payloads already travel in the original preview header; the new
fixture proves rendering with system fonts disabled. Original opaque/malformed
records remain preserved. Attachment inspection/editing is now validated in the separate
[authoring increment](style-authoring.md); deliberate conversion remains pending.

## Reproduction and evidence

Run the common native policy test from CONTRIBUTING. On the development Mac:

```sh
clang++ -std=c++17 -fobjc-arc -Wall -Wextra -Werror -I/opt/homebrew/include tests/native/mac-font-fallback-test.mm -framework Foundation -framework CoreText -o /tmp/vsf-mac-font-fallback-test
/tmp/vsf-mac-font-fallback-test
cargo test --locked -p visualsubforge embedded_font_header_and_source_are_preserved
cargo test --locked -p visualsubforge native_font_preview_sources -- --ignored --nocapture
export PKG_CONFIG_PATH=/absolute/native/build/meson-uninstalled
clang++ -std=c++17 -fobjc-arc -Wall -Wextra -Werror tests/native/font-preview-probe.mm $(pkg-config --cflags --libs mpv) -framework Foundation -framework CoreText -o /tmp/vsf-font-preview-probe
DYLD_LIBRARY_PATH=/absolute/native/build python3 scripts/check-preview-fonts.py --probe /tmp/vsf-font-preview-probe --output tests/fixtures/generated/fonts/composition-new
```

The software controls compare original generic source against the fallback,
explicit and unavailable named fonts, embedded-only rendering, attachment removal
and a deliberately unavailable glyph. Images and renderer notices are separate
assertions. The probe uses production serialized packets and the production
fallback helper; it verifies each private resource is removed after exit.
Actual Mac `--ass` desktop testing additionally covers accepted-state Undo/exact
save, same-player attachment replacement, visible notices and media-session reset.
Use the native screenshot acknowledgment lane described in the desktop README;
WebKit snapshots alone omit native video composition.

Initial evidence: software `composition-v5` passes all seven controls; generic
and explicit-font images contain legible 世界, and the embedded fixture displays
its original triangle/bar outlines. Embedded-only attachment removal is black.
The helper test passes 20 independent resource lifetimes, including a second live
owner. Common notice tests cover duplicate/limit/control-character/reset policy.
Both Chromium/WebKit ASS workflows pass 8 assertions each, including escaped,
read-only session notices. The full browser suite passes 48 cases. Workspace tests
pass 301 ordinary tests; 41 frontend tests, strict Clippy/lint/format/type/IPC checks
and native-media compilation pass. Ignored probe tests are separate explicit runs.

Actual Mac OpenGL application evidence is retained in `test-results/desktop-Ek50zY`
and the corrected missing-glyph rerun `test-results/desktop-XfAmGf`. Native window
observations show generic ASS/SSA CJK, explicit fonts and the original embedded
triangle/bar shapes. Editing/Undo and exact Save As retain the attachment; replacing
the document with its attachment-free control restores ordinary letters in the
same player. A missing family and U+10FFFF produce two bounded notices without
blocking playback or saving; media replacement clears them. The first project
restore observation was black; a repeated native observation showed the expected
frame. Backend readiness alone is not evidence of visible composition.

The unchanged 10k ASS/SSA fixtures and one-second interaction budget pass: open
315/287 ms, text 54/114 ms, style 53/103 ms, timing 104/53 ms, maximum Undo 47/95 ms,
navigation 36/25 ms, typing 1/1 ms, cancellation 15/15 ms and invalidation 53/52 ms.
Tables remain bounded to 6 cue and 200 proposal rows in this run.

Exact native validation commands (with the private mpv pkg-config/runtime paths
selected as above):

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked -p media-player -p visualsubforge --all-targets --features desktop-e2e,media-prototype -- -D warnings
npm test
npm run lint
npm run format:check
npm run build
npm run ipc:check
npm run test:browser
npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype --config /absolute/fonts/tauri.native.json -- --locked
VSF_ASS_DESKTOP_CAPTURE=1 node tests/desktop/run.mjs --ass --ass-performance --binary target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
VSF_ASS_DESKTOP_CAPTURE=1 VSF_ASS_DESKTOP_CAPTURE_LABELS=ass-missing-font node tests/desktop/run.mjs --ass --binary target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
```

Use pinned Node directly with `DYLD_LIBRARY_PATH` for native tests. The ignored
test config uses the ordinary bundle identifier for native window inspection.
Application SHA256 is `9a04e3f6fba29505f6a7ecdcbbd5853fa2d9716e130887fc9742f855d8573d47`.
The second run's `native-runtime.json` identifies 46 loaded images, none unavailable,
including the exact application and private libmpv. Normal exit code 0 removes its
private font directory. The first run deliberately SIGKILLs between workflows
for recovery testing and leaves a tiny font-link directory, as documented above.
This evidence does not validate ANGLE, release packaging or Windows/Linux rendering.

Primary references: [mpv font options](https://mpv.io/manual/master/#options-sub-font-provider),
[libass issue 845](https://github.com/libass/libass/issues/845), and the pinned
runtime's `options/path.c`, `sub/ass_mp.c` and `sub/sd_ass.c`. Software/probe checks
do not establish installation, physical GPU/audio or other-platform acceptance.
