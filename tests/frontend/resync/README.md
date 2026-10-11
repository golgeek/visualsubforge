# Resync workflow checks

Run `npm run test:browser -- --grep 'workflow: resync'`, or open
`/tests/frontend/resync/` on the development server. The existing runner checks
both Chromium and WebKit, counts completed assertions and rejects uncaught errors.

This fixture runs the real compiled Svelte owners and App with controlled IPC.
It covers draft apply/discard/cancel, late starts and picker replies, stale media/
document captures, review cancellation, page disposal, serialized acceptance,
late cancellation after a commit, failed acceptance, and workspace disposal.
The App portion checks native-occlusion ordering, incomplete proposals, paged
old/new timings, one Undo/Redo, protected Save As cancel/failure, and the
save-subtitles-before-project continuation. Its filesystem and media are models;
Rust/FFmpeg tests and actual native workflows supply separate evidence.

`/tests/frontend/resync/?review` leaves an interactive, modeled proposal open for
layout and keyboard inspection. It intentionally does not report automated PASS.
The review tables scroll independently and can receive keyboard focus. The whole
dialog scrolls in small viewports. No native player, file chooser or disk writes
are exercised by either browser mode.
