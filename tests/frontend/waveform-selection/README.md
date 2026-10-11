# Waveform playback-range interaction regression

With pinned Node, run `npm run dev` and open
`http://localhost:1420/tests/frontend/waveform-selection/`.
Expect **PASS: all 22 interaction checks**, with no FAIL row.

The production Waveform component receives synthetic pointer/keyboard events;
the harness uses actual Svelte and ResizeObserver lifecycle delivery. Pointer
capture ownership is modeled because synthetic pointers cannot acquire native
capture. Checks cover right-click rejection, local preview, one final release,
Escape/blur/capture loss/resize/viewport/media-duration/disable/unmount
cancellation, unrelated pointers, and ordinary seek behavior. Accepted ranges
and seek callbacks must remain unchanged after cancellation. Wheel checks cover
pan, anchored zoom, cancellation of a pending drag and disabled navigation.
First cue clicks delegate selection to the parent; overlapping intervals prefer the selected cue,
then the last painted cue. Drags starting inside cues still select a range.
Gesture callbacks bracket viewport freezing and release it on cancellation.

This is component behavior evidence, not native input delivery, Rust document
history or audio continuity. The page does not invoke Tauri or write files.
`npm run check`, lint and format checks include these sources; `npm test` runs
the separate pure TypeScript suite, not this page.

A click inside an already selected cue with its exact bounds selected instead
seeks to the clicked time. Selecting another interval restores first-click cue
selection. The parent integration fixture verifies the ensuing range update.
