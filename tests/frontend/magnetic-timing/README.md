# Magnetic timing UI regression

With the pinned Node toolchain, run `npm run dev` and open
`http://localhost:1420/tests/frontend/magnetic-timing/` in an ordinary browser.
The page reports all 14 magnetic timing checks. It mounts the production App, waveform and
cue handles, with modeled IPC and synthetic pointer capture. It covers snapped
cue commits/live-loop previews, cancellation, exact keyboard/bypass behavior,
forward/reverse new ranges, settings layout, preset IO and project persistence.
Rust unit tests cover actual preset/project storage; the [current status](../../../docs/status.md)
keeps browser checks separate from historical native dragging/dialog trials. This page does not prove native
capture, undo storage, disk IO or audio timing.
