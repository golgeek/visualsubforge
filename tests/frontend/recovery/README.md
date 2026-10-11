# Recovery response ordering regression

With the pinned Node toolchain, run `npm run dev` and open
`http://localhost:1420/tests/frontend/recovery/` in an ordinary browser. The page
must report **PASS: all 4 recovery response ordering checks** in about 20 seconds.
Reload reruns it. `npm run check`, lint and formatting include this harness;
`npm test` does not run browser pages.

The fixture mounts the production App and uses its typed client, real polling
intervals, DOM buttons and Svelte lifecycle. Only document status, recovery status
and recovery discard IPC responses are mocked. It refuses to replace IPC in a
native Tauri process; no subtitle or recovery files are read or written.

A recovery poll starts while Discard is awaiting a delayed response. After
successful discard, both a late list and a late error must be ignored. A later
current poll must still discover a new entry. Failed discard must retain the
entry and report its error. The app is unmounted after each case and the mock
is removed.

Before the fix, the production component failed with
`Late poll resurrected the discarded entry`. This is frontend reconciliation
coverage, not storage durability, native IPC or desktop lifecycle acceptance.
