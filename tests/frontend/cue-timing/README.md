# Waveform handle interaction regression

With the pinned Node toolchain, run `npm run dev`, then open
`http://localhost:1420/tests/frontend/cue-timing/` in a browser. The page must
report **PASS: all 21 interaction checks**, with no FAIL row. Reload reruns it.
`npm run check` and `npm run lint` include this harness; `npm test` runs the
separate pure TypeScript tests, not this browser page.

The harness mounts the production CueTiming component and uses synthetic DOM
keyboard/pointer/focus events plus the real ResizeObserver and Svelte mount lifecycle.
It checks repeated-arrow preview grouping, Escape/blur/resize/unmount
cancellation, ignored repeat events after cancellation, and disabled handles. Pointer checks cover grab offset, click jitter, return to
the original time, final release coordinates, both endpoints, switching focused handles and cancellation.
Six ownership cases verify that cancellation/capture loss from another pointer
or handle cannot cancel the active gesture, and pointer cancellation cannot end
a held-key gesture. Synthetic pointers model capture ownership on the element; this is handler
coverage, not OS capture delivery.
Parent callbacks record preview and commit requests; they do not simulate Rust
history or prove physical keyboard delivery, pointer capture, native webviews,
video presentation, or audio behavior. Those require the separate desktop checks.

On 2026-10-04 the browser harness reproduced a cancelled held-arrow gesture
restarting on a later repeat and committing on keyup. The production guard now
requires a fresh, non-repeat keydown to start a gesture. All seven cases pass in
the Codex in-app browser after the fix. Test fixture data is disposable and never
calls Tauri or writes subtitle files.

For native input-routing diagnosis, append `?manual` to the page URL. The manual
fixture mounts the same production control, displays accepted versus preview
endpoints and commit/cancellation counts, and lists the last 20 keyboard events
with modifier flags and `defaultPrevented`. It observes input without synthesizing
it. This distinguishes a delivered key from a claimed shortcut outcome; it still
does not validate Rust history. A bundled copy can be embedded in an isolated
native shell with a separate application identifier, without changing production
application state or dependencies.
