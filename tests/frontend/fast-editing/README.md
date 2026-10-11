# Fast cue editing regression

Run `npm run dev` with pinned Node, then open
`http://localhost:1420/tests/frontend/fast-editing/`.
Expect **PASS: all 23 fast-editing checks**.

The real App and client use modeled IPC in an ordinary browser. Checks cover
immediate text focus, draft preservation before creating another cue, transport
while typing, Escape/Enter focus, held-Space timing with repeats, Escape/blur
cancellation, creation from a waveform selection, targeted row deletion without
seeking, waveform cue menu keyboard cancellation, short-Space pause, paused-Space start,
consecutive holds with retained waveform focus, and explicit Enter/text/Escape
transitions. Follow checks cover scrolling before/across the old viewport edge
during a held cue, waveform/list clicks seeking to cue starts while preserving playing/paused
state, and Follow disable/resume.
No files are written.
This does not prove native keyboard delivery, physical press/release latency,
Rust undo, or audio/video alignment. Separate pure tests cover stale document,
media, pause, seek, and backward-clock cancellation.

Cue selection now replaces the previous playback range. A second waveform click
inside the exact selected cue seeks to the pointer without changing that range.
Timing previews update an enabled loop before document commit; Escape restores
both range and loop, release makes one accepted edit, and Clear loop prevents
subsequent previews from reactivating it. Delayed transport/coalescing and failure
behavior also have pure `live-loop.test.ts` coverage.
