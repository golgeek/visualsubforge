# Real desktop workflow test

This lane runs the Svelte editor in Tauri's actual webview, using the generated
client and real Rust document, persistence and recovery operations. It complements
the controlled-response browser tests in `tests/frontend`; it does not replace them.

From the repository root with the pinned Node/Rust toolchains and ordinary Tauri
development prerequisites:

```sh
npm ci
npm run build:desktop-test
npm run test:desktop
```

On headless Linux, install `xvfb`, `xauth` and `dbus`, then run:

```sh
GDK_BACKEND=x11 xvfb-run -a -s '-screen 0 1280x800x24 -nolisten tcp' dbus-run-session -- npm run test:desktop
```

`test:desktop -- --binary /absolute/path/to/visualsubforge` selects another
instrumented executable. The default is `target/debug/visualsubforge` (`.exe` on
Windows). The test build replaces that debug executable; rebuild normally before
using it as an ordinary editor.

## What it verifies

The runner opens a BOM/CRLF Unicode SRT, commits a text edit, checks stable cue
identities and complete Undo/Redo restoration, and exercises cancelled and failed
Save As before saving and reopening an exact-byte copy. It waits for a real
recovery checkpoint, abruptly terminates its own app, modifies the saved file to
simulate an external editor, then relaunches and uses the recovery UI. It verifies
that the external edit survives a rejected save, saves recovery elsewhere and
requests normal application shutdown.

Each run retains `test-results/desktop-*`: its inputs/outputs, isolated recovery
data, per-process logs, receipt and screenshot (plus DOM on failure). The runner
has bounded waits and terminates only processes it spawned. CI additionally caps
the run step at five minutes. Recovery after process termination does not prove
power-loss durability.

## The test boundary

The explicit `desktop-e2e` Cargo feature enables pinned
`tauri-plugin-wdio-webdriver` 1.5.0; the Node runner uses pinned WebdriverIO 9.31.9.
The embedded driver serves standard WebDriver on a loopback port chosen for the
run. No external driver, IPC-mocking plugin or native media SDK is required.
Normal builds exclude the driver and the test module.

`tauri.test.json` gives the test app a separate identifier and permits test code
to emit the existing workspace-action event. The runner emits those menu actions
and uses ordinary webview controls; document writes still travel through the real
frontend client. Read-only status probes call the real Rust command.

Two services are deliberately supplied by the harness: a queue of file selections
(including cancellation) and a unique app-data directory. `desktop_test.rs`
requires the absolute `VSF_E2E_ROOT`, confines queued paths to it, and redirects
settings/recovery there. These are fixture constraints, not an OS sandbox.
There are no mock document replies, test-only document commands or alternate
accepted-state owners. The optional driver lives for the app process; the runner
owns process cleanup and retains evidence even on failure.

This lane does **not** exercise native menu clicks, native file dialogs, media
playback, GPU/audio output or the resync proposal walkthrough. Those have separate
backend/browser tests and native acceptance checks. Do not distribute the
instrumented executable as a production app.

Local evidence on 2026-10-11: passes on macOS ARM64 and Ubuntu 26.04 ARM64 under
Xvfb. Windows and hosted CI execution remain unverified. The first Mac/Linux runs
incorrectly expected ordinary recovery to prohibit any reuse of its unchanged
source; the corrected scenario tests the actual external-edit conflict guarantee.
Resync's stricter distinct-file rule is covered separately.
