# Untitled subtitles and waveform navigation

With pinned Node, run `npm run dev` and open
`http://localhost:1420/tests/frontend/new-subtitles/` in an ordinary browser.
Expect **PASS: all 6 new-subtitle/navigation checks**.

The production App and typed client run against explicitly modeled document,
media and chooser IPC. Tests create the first cue with media but no SRT, cancel
first Save, retain state on failure, adopt/reuse the first successful destination,
pan without seeking and reenable Follow playback. No files are written. The
fixture refuses to run inside a real Tauri process and removes its mocks on exit.
Rust tests independently validate actual document, persistence and recovery
behavior; this browser fixture does not establish native file-picker delivery.
