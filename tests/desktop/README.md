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

`npm run test:desktop -- --binary /absolute/path/to/visualsubforge` selects another
instrumented executable. The default is `target/debug/visualsubforge` (`.exe` on
Windows). The test build replaces that debug executable; rebuild normally before
using it as an ordinary editor.
It also builds `vsf-cli` for the executable parity check. `--cli /absolute/path/to/vsf`
selects a matching CLI; by default the runner uses `target/debug/vsf` (`.exe` on Windows).

## What it verifies

The runner opens a BOM/CRLF Unicode SRT, commits a text edit, checks stable cue
identities and complete Undo/Redo restoration, and exercises cancelled and failed
Save As before saving and reopening an exact-byte copy. It waits for a real
recovery checkpoint, abruptly terminates its own app, modifies the saved file to
simulate an external editor, then relaunches and uses the recovery UI. It verifies
that the external edit survives a rejected save, saves recovery elsewhere and
requests normal application shutdown. It also exercises quality profile selection,
customization, cancelled/invalid imports, export/reimport and timing-only imports
through the real Settings UI. An actual failed settings-file replacement preserves
accepted rules; retry persists them, a stale dialog is rejected, and a second
process restores the exact saved profile. All these operations must leave the
loaded subtitle document and Undo history unchanged.

The quality workflow then enables accepted/draft metrics and Problems without
media, verifies protected Cancel/Discard navigation, and checks that the list
and editor retain usable space. It exports Rust's accepted profile, executes
`vsf check` on the same SRT, and compares the profile/hash, every metric/finding
and affected-cue count. The partial profile must remain explicitly incomplete.
Screenshots cover both the panel controls and scrolled findings; the JSON parity
receipt preserves both actual outputs. Neither checking nor draft metrics may
change the accepted document, history or save eligibility.

Minimum-gap review then verifies protected draft cancellation/discard, a read-only
preview, explicit no-media acknowledgment and atomic acceptance. One Undo restores
every cue and the original dirty/history state; Redo reapplies the plan. The normal
save/recovery checks run afterward. The explicit history buttons must use document
history even when a text field retained focus during the click; menu/keyboard Undo
still uses the focused field's local history. Modal closure waits for native
occlusion/focus restoration before further workspace actions.

Each run retains `test-results/desktop-*`: its inputs/outputs, isolated recovery
data, per-process logs, receipt and screenshot (plus DOM on failure). The runner
has bounded waits and terminates only processes it spawned. CI additionally caps
the run step at five minutes. Recovery after process termination does not prove
power-loss durability.

Startup waits for the actual application's document title before the first
script evaluation, then separately requires Tauri APIs, the editor and completed
workspace initialization. The embedded driver's window-ready status can precede
initial navigation; its multi-evaluation script result can be lost across that
navigation. Document title/URL evidence is retained for each launch. This avoids
starting a script in the initial blank document without retries or longer script
deadlines. The real Mac workflow passes locally with this readiness check;
the recurring hosted second-launch timeout still needs replacement-run evidence.

## The test boundary

The explicit `desktop-e2e` Cargo feature enables pinned
`tauri-plugin-wdio-webdriver` 1.5.0; the Node runner uses pinned WebdriverIO 10.0.2.
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

Local evidence on 2026-10-11: the workflow passes on macOS ARM64, Ubuntu 26.04
ARM64 under Xvfb, and Windows 11 ARM64. Hosted default-app execution also passes
on macOS 14, Windows Server 2022 and Ubuntu 24.04 at `ce43559`
([run](https://github.com/golgeek/visualsubforge/actions/runs/38167423845)); that
run's separate modeled browser job failed and was corrected subsequently.
The first Mac/Linux runs
incorrectly expected ordinary recovery to prohibit any reuse of its unchanged
source; the corrected scenario tests the actual external-edit conflict guarantee.
Resync's stricter distinct-file rule is covered separately.

The expanded quality Settings/checker/CLI-parity/restart workflow passes on macOS ARM64 on
2026-10-17; this addition has not yet been executed on Windows/Linux. The pinned
embedded driver uses JavaScript clicks, so select options require explicit DOM
input/change events in this harness. This verifies the real component/IPC path,
not physical keyboard or pointer delivery. Retained current evidence is copied
under `tests/fixtures/generated/quality/checks-desktop-final/`; the expanded gap
workflow is under `quality/gaps-desktop-final/` (2026-10-17, macOS ARM64).

The first Windows run reached the save-failure check, then failed because Rust's
canonical path had the Windows extended-length prefix. The runner now normalizes
that representation with Node's `toNamespacedPath`, retaining a complete path
comparison. No product save behavior changed.

Dependency review replaced the initial WebdriverIO 9.31.9 trial with 10.0.2,
removing the affected archive/proxy dependency chain reported by `npm audit`.
The runner follows v10's documented async `execute` API; it still uses the same
standard WebDriver endpoint on the embedded plugin. The existing `source-map-js`
1.2.1 lock entry was also updated to the compatible 1.2.2 security patch.
`npm audit --json` reports zero advisories on 2026-10-11; this is a dated registry
check, not proof of vulnerability-free dependencies. Upgrade these pins with the
same real-app checks on each OS. See the
[v10 changes](https://github.com/webdriverio/webdriverio/releases/tag/v10.0.0),
[archive advisory](https://github.com/advisories/GHSA-jmr9-qjv8-65gv) and
[source-map advisory](https://github.com/advisories/GHSA-68fv-2mgg-jv7q).
