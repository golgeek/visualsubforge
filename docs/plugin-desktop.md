# Desktop subtitle plugins

Stage7 is complete within the initial checker/fix subset. The desktop controls compose the bounded Rust owner
and helper. Eight actual Mac format/API workflows and a separate real macOS
chooser workflow pass, along with ten complete desktop10k workloads. Windows/Linux
ARM64 automated suites and complete subtitle-only plugin editor workflows pass.
The [historical checker/fix comparison](plugin-api.md#historical-vss-comparison--2026-10-18)
also passes with documented intentional differences; ADR0008 is accepted. Physical
GPU/audio, real guest OS choosers and installation acceptance remain separate.

## Workflow

Open **Tools → Subtitle Plugins…** or **Subtitle plugins…** above the cue list.
Choose a local package's `manifest.json`; examples are in `examples/plugins/`.
There is no package installer or automatic discovery. See the [API matrix](plugin-api.md)
for manifest layout, supported modern/VSS APIs, dependencies and limits.

Resolve an unapplied cue draft with **Apply cue draft** or **Discard cue draft**.
Choose all cues or the current multi-cue selection. **Run checker** is read-only;
**Preview plugin fixes** prepares a validated proposal without editing subtitles.
The dialog shows the plugin's name, effective preferences/settings, scene coverage,
findings and logs. All script text is rendered literally, never as HTML.

Review raw before/after text and timing. Findings and changes have independent
50-row pages. Long text has a visibly truncated preview and a full-text reader
with bounded UTF-8 chunks. Changes outside the requested scope are labeled:
supported VSS fixes may edit neighboring cues. **Accept plugin changes** commits
every proposed row, including all pages and outside-scope changes, as one Undo
step. Ordinary save, Undo/Redo and recovery then handle the accepted document.
No file is saved merely by running or accepting a plugin.

The advanced form overrides declared settings using their exact name and scalar
type. First run a check to see the effective defaults, then use **Edit effective
settings** to populate the form. Unknown names or changed types are rejected by
the host. A blank CPS target uses the active profile; if that profile has no CPS
target, the host requires an explicit override. The VSS RS target index is
unavailable unless explicitly supplied. Scene filter offset is an explicit value,
separate from the profile's before/after scene offsets; its initial value is0ms.

The current selection is available as plugin context even for whole-document
scope. The optional playhead is available only with loaded, paused, non-seeking
media; otherwise it is explicitly unavailable. Pause before opening the dialog
to provide a stable position. Playback is not automatically paused or resumed.
Only completed standard/reduced scene results provide cuts; coverage is shown.
Changing the document, selection, draft, package, options, profile, media, scenes
or provided cursor invalidates the review. Run again before accepting.

Cancel requests helper termination and retains a visible terminal phase. Closing
cancels pending work, including a start whose job ID arrives after closure. A
native chooser, draft application or acceptance must finish before closing. If
the acceptance reply fails, accepted state is refreshed because Rust may already
have committed. If that refresh also fails, the dialog blocks continuation and
offers **Refresh accepted subtitles**; it never silently assumes rollback or
resubmits the fix. Confirm actual accepted text/history after a lost reply.

## Ownership and validation

`PluginSession` owns form drafts, the job ID and bounded report projections.
Request epochs reject late poll/page/text replies and cancel late starts. It
compares the captured accepted snapshot and explicit invocation context before
adopting results or accepting. `DocumentSession.acceptPlugin` is the sole frontend
adoption path and serializes with all other subtitle commands. Rust independently
validates the private proposal, package identity, revision and media/scene context
before its atomic history transaction; frontend guards do not replace it.

App uses the existing authoring modal gate, video occlusion and focus restoration.
The native menu dispatches the same operation as the toolbar. Native JSON file
filters are defined for Mac/Windows dialogs and the Linux portal, and file IO
remains behind the Rust package boundary. The installed sibling CLI runs the
helper; the [host guide](plugin-host.md) explains execution and resource limits.

Ten browser harness checks run in both Chromium and WebKit: picker cancellation,
draft gating, literal findings/tags/logs, paging, typed defaults/overrides, complete
Unicode text chunks, outside-scope review, accepted snapshot adoption, context
and late-result races, cancellation, close during startup, and lost-acceptance
reconciliation including failed refresh/retry. The App harness separately checks
toolbar/native-menu dispatch, draft preservation, video occlusion and focus.
These use modeled IPC; real execution/format/history evidence remains in the
separate Rust owner/API/helper tests. They do not prove native plugin workflows.

Evidence and exact final checks are recorded in `test-results/plugin-ui-20261018/`
and the [host guide](plugin-host.md). The initial browser failure came from a test
assertion querying the Accept button before asynchronous review rendering; that
failure is retained and the harness now waits for the actual review. ADR0008
remains Proposed pending the complete stage7 acceptance.

## Native workflows and real chooser — 2026-10-18

The `--plugins` desktop lane passes modern/VSS check and fix workflows across
SRT, ASS, SSA and WebVTT in the actual WebView with the installed sibling CLI.
Each case protects drafts, checks without mutation, reviews literal text/tags,
accepts one transaction, verifies every cue and format property, performs one
Undo/Redo, and checks cancelled/failed Save As, byte-exact save/reopen and recovery
after abruptly terminating only its owned app. Original sources remain unchanged.
The first case also observes a live looping helper, cancels it through the UI and
verifies reaping; thrown fixes and package changes after review preserve exact
accepted state. Every format reaches a native rendered frame acknowledging its
accepted generation/revision. The lane supplies file choices and DOM events.

`test-results/desktop-Mqq8Ir/plugins.json` and `receipt.json` retain all eight
passing cases and normal code0 shutdown. Independently observed native-window
captures show raw-tag review, modal video occlusion and accepted tagged-SRT/CJK
rendering. WebVTT's existing CSS/region preview differences and an emoji missing
glyph remain visible; preservation does not imply browser-equivalent rendering.
Native PNGs and full media/document snapshots accompany the report.
A final complete rerun after correcting a report field also passes in
`test-results/desktop-HejUnC/`; native status records configured video output and
hardware decoder fields instead of an unavailable backend field. No product code
changed. Node syntax, pinned Prettier/ESLint and diff whitespace checks pass.

A separate **non-WebDriver** bundle passes actual macOS Open Subtitles, JSON
manifest selection and Save As sheets, including Unicode paths and cancellation.
An invalid manifest produces a useful error; an unapplied draft blocks execution,
survives chooser cancellation/dialog close, and regains focus. The native Tools
menu and toolbar both work. An independently authored VSS plugin checks one
selected cue, then proposes changes to it and its neighbor. The second row is
labeled **Outside selected scope**. Acceptance changes both; one Undo restores
both and the saved state, and one Redo reapplies both. Native Save As writes a
separate exact-byte output, retaining BOM, CRLF, Unicode and ASS/HTML tags. The
source remains unchanged, and normal Cmd-Q exits code0. Receipt and observations
are in `test-results/plugin-picker-20261018/`.

Identities on Apple M3 Max, macOS27.0 build26A428, Rust1.98.1 and Node24.21.0:

| Artifact | SHA256 |
| --- | --- |
| Native test executable | `568294c89666aba1bdd55b2135d745bcbb2eaaed706fe33dae9eb680fdd86be4` |
| Normal picker executable | `58997891ca40b566d1ab1f6a87b46dfbab221c3fa792a6b3ebbf61fd14bc71ce` |
| Sibling CLI in both bundles | `a08952ff324f5ca22258243c2a26d46d04f42e52442abeec7806177f0f9e3054` |
| Native chooser source (125 bytes) | `4fe27f41f819ca203c4cb8b60dbeee4d218e5f33669b317bc1790c2a951c0fb9` |
| Native chooser output (136 bytes) | `663554449841d98931857b522352f77d14aec17072fa627fa5f314fd61878872` |

Build logs/configurations are in `test-results/plugin-native-20261018/` and
`test-results/plugin-picker-20261018/`. With pinned toolchains, reproduce the
instrumented lane using the private runtime identified in the [Mac build guide](build-macos.md):

```sh
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app \
  --features desktop-e2e,media-prototype \
  --config "$PWD/test-results/plugin-native-20261018/tauri.json" -- --locked
cargo build --locked -p vsf-cli
# Copy the app to an isolated test location and copy target/debug/vsf into its
# Contents/MacOS directory before launching. Keep both binaries together.
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  node tests/desktop/run.mjs --plugins \
  --binary "$PWD/test-results/plugin-native-20261018/VisualSubForgePlugins.app/Contents/MacOS/visualsubforge"
```

The normal picker build uses only `--features media-prototype` with its separate
identifier/configuration; it has no test server or supplied chooser queue. The
native lane's optional capture handshake is documented in the [runner guide](../tests/desktop/README.md#subtitle-plugins).
Helper process observation uses Unix `ps` or a prestarted Windows CIM observer.
The observer never terminates plugin helpers; UI cancellation must make the
production owner stop and reap the helper. A separate real-helper observation
test passes on Mac, Ubuntu ARM64 and Windows ARM64. Windows/Linux full plugin UI
acceptance remains pending as recorded below.

Earlier failures are retained: `desktop-bm9Oob` started a review while automatic
scene analysis was still changing context, so stale protection invalidated it.
The harness now waits for completed scene context and a settled paused cursor.
`desktop-cTi2cM` passed cancellation/error controls, then hit an incorrect singular
DOM selector for two review articles. The assertion now checks both rows. No
product guard, execution deadline or fixture content was relaxed. Native workflow
evidence alone does not establish desktop scale or historical/platform acceptance;
see the scale results below. Physical Windows/Linux GPU/audio and release readiness
remain separate gates.

## Subtitle-only VM workflow checkpoint — 2026-10-18

The explicit `--plugins-editor` mode runs the same eight modern/VSS ×
SRT/ASS/SSA/WebVTT workflows without media. It retains all review, source, history,
failed-save, exact-output, reopen, recovery and live-helper cancellation checks.
Windows paths are compared using the existing full canonical-path normalizer;
recovery selection uses the backend's authoritative source path.

The source archive combines base `615030d` with the four harness files listed in
`test-results/plugin-platform-20261018/desktop-source-manifest.json` (790 verified
files). Archive SHA256:
`c0c7cb21519572e822f1d799102ddd9631f174b8d57213023bdced46e0018174`.
Both guests pass `npm run build:desktop-test` and the one real-helper observer
test. Their next commands fail before entering the plugin cases:

```sh
# Windows ARM64
node tests/desktop/run.mjs --plugins-editor
# Ubuntu ARM64, GDK_BACKEND=x11
xvfb-run -a -s '-screen 0 1280x800x24 -nolisten tcp' dbus-run-session -- \
  node tests/desktop/run.mjs --plugins-editor
```

| Instrumented desktop | SHA256 |
| --- | --- |
| Ubuntu ARM64 | `781ed8733be7b348f6bb72851025301ee9d1018ed3977e49b6f5e54e22628284` |
| Windows ARM64 | `31c54250fd41d830f00a6b0b0a538b38ee9e9281087b440c15a8441f8dd6a5ab` |

Sibling CLI hashes are unchanged from the [automated VM record](plugin-host.md#vm-automated-validation--2026-10-18).

Both fail the existing `Problems squeezed out the cue list` assertion in
`qualityChecking`: the visible list must retain at least two 32px rows. Retained
screenshots show the crowded default-size workspace; the table allocation still
uses a fixed minimum despite wrapped platform-dependent control heights. The
next correction must preserve the layout requirement, then rerun both complete
lanes. No plugin UI, GPU/audio or installation pass is claimed for these runs.
Logs, manifests, receipts and screenshots are retained under
`plugin-platform-20261018/{linux,windows}-desktop-*`; Windows evidence was copied
back from the owned Downloads staging folder.

The original Mac native `--plugins` regression passes all eight cases, cancellation,
exception rollback, package revalidation and normal code0 shutdown in
`test-results/desktop-6YUUrg/`. It uses the unchanged native executable/CLI hashes
listed above and the same private runtime. Local JavaScript syntax, ESLint and
Prettier checks pass. Historical VSS-host comparison and stage8 remain open.

## Wrapped toolbar correction and complete VM editor acceptance — 2026-10-18

The shared workspace now measures transport, cue-operation and tool/following
bar heights instead of assuming a fixed allowance. The table keeps its column
header and two full cue rows; preview and editor minima still apply. The existing
ResizeObserver action commits rounded-up measurements on the next animation frame
and disconnects/cancels on unmount. These are transient layout measurements only;
document ownership, IPC, plugin limits and native resource ownership are unchanged.

A production-App regression naturally wraps controls at1100→820→1100px while
Problems and metrics are visible and following is suspended. It requires two
complete cue rows, a usable textarea and no overlap with toolbars/editor/footer.
Both Chromium and WebKit fail on the old implementation at820px, then pass after
the correction. The original failure traces are retained in
`test-results/plugin-platform-20261018/layout-before-browser/`.

The complete subtitle-only lanes now pass on both guests: eight modern/VSS ×
SRT/ASS/SSA/WebVTT cases each, all review rows, exact source/output/format state,
one Undo/Redo, cancelled and failed saves, reopen and durable crash recovery.
Live looping helper observation, UI cancellation/reaping, exception rollback and
package revalidation pass. Both applications exit normally with code0. Supplied
chooser queues and generated DOM events do not prove real OS-sheet or physical
input behavior; no media is loaded in these guest lanes.

| Run | Evidence directory | Desktop SHA256 | Observed cue-list height |
| --- | --- | --- | --- |
| Ubuntu ARM64/Xvfb/X11 | `linux-desktop-layout-results/desktop-OdOvOn` | `7cf8b0694a32a4646f176d0ba5009300e61a229385f0a7ef4b2fb123533d607a` |64px|
| Windows11 ARM64/WebView2 | `windows-desktop-layout-results/desktop-tNC3fd` | `a3d05536e427bc2b972de5eb455f906923d6fb6762d52fe0de7dc8396f7e5354` |64.5px|

Paths above are under `test-results/plugin-platform-20261018/`; sibling CLI hashes
remain those in the automated VM record. Both guests verify790 source-file hashes
from base779eab8 plus `src/App.svelte` and the browser regression. Archive SHA256:
`fc88263e06026cdf5e432ff7c23e9484c31bea87c5ebb55a4a394d469790a94a`.
The `*-desktop-layout-receipt.json` files retain exact build/run commands, tool
paths, durations and binary identities. Commands are the same as the preceding
checkpoint; no assertion, fixture, helper deadline or test timeout was relaxed.

A fresh Mac native build also passes all eight complete workflows and normal
shutdown in `test-results/desktop-vz2qP0/`, including acknowledged preview updates.
Its SHA256 is `d97e25c821cf4f0c134b137e53070cbcbf816f6cbf97923014b9753ae3233056`;
the sibling CLI remains `a08952ff324f5ca22258243c2a26d46d04f42e52442abeec7806177f0f9e3054`.
Build uses the same native configuration/runtime above, with an isolated
`CARGO_TARGET_DIR=test-results/plugin-layout-native-build/target`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_BUILD_JOBS=4` to bound
rebuild storage. The unoptimized dev profile has no debug symbols. Full commands
and identity are in `layout-mac-native-build.log` and
`test-results/plugin-layout-native-build/identity.json`. The native run uses
`--plugins --binary` pointing to that target's bundled executable and the same
process-local private runtime.

Local `npm run build`, `npm run lint`, `npm run format:check`, `npm test` (44 tests),
`npm run test:browser` (60 cases), and `git diff --check` pass. The focused browser
command is `npm run test:browser -- --grep 'workflow: quality-checking'`; its
before/after logs and full-suite logs are retained with the VM evidence. No Rust
code or IPC contract changed, so the previously passing workspace suites were not
rerun for this layout change. The subsequent complete plugin10k regression
`desktop-jweQbK` passed nine workloads, then exceeded the unchanged1000ms helper
deadline during the fourth fix preview of timed-WebVTT/VSS. This failure is retained
in `layout-plugin-scale.log` and that run's report; the earlier complete pass below
does not establish current regression acceptance. The worker transport and VSS
newline fast paths are corrected and the complete Mac rerun passes, as recorded
below. Windows/Linux reruns of the corrected worker also pass.
Historical-host comparison remains pending because guest UI input is unresponsive.

## Plugin deadline correction — 2026-10-18

The retained `desktop-jweQbK` failure above triggered a measured correction;
no execution limit, workload, assertion or preparation budget changed. The helper
now retains the engine's serialized result as validated `serde_json::RawValue`,
avoiding an intermediate Rust object tree and its reserialization. The parent
still parses the entire reply into `Value` under its existing depth/byte/deadline
bounds, and Rust independently validates every proposed edit. No JS value leaves
the helper. Only the `raw_value` feature of already-locked serde_json is enabled;
there is no dependency version change. VSS text getters/setters avoid newline
regex work when no newline conversion is needed. Unicode, literal ASS escapes,
CRLF/LF normalization, read-your-writes and bare-CR refusal remain tested.

Fifteen real-helper API tests and eleven worker tests pass; one separately
acquired historical-script test remains ignored in that command. New transport
coverage checks JSON escaping/Unicode/nested values and rejection of excessive
nesting by the parent. Twenty diagnostic runs using the actual timed-WebVTT10k
input and current adapter preserve all 10,000 expected changes each. Same-profile
control p95 is 655.314 ms; corrected p95 is 476.105 ms (maximum 705.895 ms). This helper
diagnostic retains the 1000 ms engine limit but does not run the parent watchdog;
it is not desktop acceptance. Raw JSON alone measured p95 587.717 ms. All requests,
binary hashes, failed probes and measurements remain in
`test-results/plugin-platform-20261018/`.

Fresh native workflows pass all eight API/format combinations in
`test-results/desktop-NeysT7/`. The complete scale rerun passes in
`test-results/desktop-mpaa7v/`: ten combinations, 4000 full review pages, 50 cycles,
exact history/save/reopen/recovery, cancellation/stale refusal, and normal code0
shutdown. Maximum interactions and complete asynchronous preparations are:

| Fixture | Modern interaction / preparation ms | VSS interaction / preparation ms |
| --- | ---: | ---: |
| Dense tagged SRT | 689 / 964 | 678 / 1055 |
| ASS | 362 / 1001 | 357 / 1012 |
| SSA | 319 / 1001 | 317 / 1004 |
| Styled WebVTT | 345 / 1203 | 357 / 1418 |
| Timed WebVTT | 323 / 1063 | 329 / 1145 |

The unchanged interaction/preparation budgets are 1000/10000 ms; helper limit is
still 1000 ms. The 4609 RSS samples peak at 340,230,144 bytes for app plus direct helper,
excluding WebKit; this is descriptive and does not replace the separate pure/API/
CLI memory gates. Complete records and compact results are in
`deadline-scale-summary.json`; native executable and sibling CLI SHA256s are
`01ab7fb0777c7d90a517779de97d1b13f8cced6109e6b5b36050e1cb3ffd3089` and
`7fd26b50249333f84bded8953881a335d18ea9a285d5edeef312246777d21b4b`.

Build uses the same isolated dev target/profile, native features/configuration and
private runtime as the layout increment above. Run `--plugins` and
`--plugin-performance` against that rebuilt bundle after copying its matching
`target/debug/vsf` alongside the app executable. Logs are
`deadline-mac-native-{build,workflow,scale}.log`; `deadline-native-identity.json`
pins source patch and binaries. Focused command:

```sh
CARGO_TARGET_DIR="$PWD/test-results/plugin-layout-native-build/target" \
  CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=4 \
  cargo test --locked -p vsf-plugin-host --test worker --test subtitles -- --test-threads=1
```

The full local suite passes 503 Rust tests/32 ignored, strict Clippy, Rustfmt,
Prettier, ESLint,44 frontend tests, strict frontend build, generated IPC and the
default desktop build. Eleven zero-test harnesses are excluded. The separately
hash-verified archived overlap checker passes again, including all three supported
integer-safe fix modes and Undo. Exact commands/results are in
`deadline-local-checks.json`, its numbered logs and `deadline-legacy-rule.log`.
No browser component changed in this correction; the60-case layout regression
baseline remains separate evidence, not a claimed rerun.

Ubuntu ARM64 passes505 Rust tests/33 ignored; Windows ARM64 passes493/32 ignored.
Both pass strict Clippy, the strict frontend/instrumented desktop/CLI builds and
all eight subtitle-only plugin workflows, including live-helper cancellation/
reaping, rollback, exact history/save/reopen/recovery and normal code0 shutdown.
Eleven zero-test harnesses per guest are excluded. Both verify all790 source-file
hashes before execution: base564f897 plus the recorded correction, archive SHA256
`b40b353b6dd627a05cc50c26ce95d5c4ce76f11919540b313e49dd42dce4adf4`.
Guest commands are:

```sh
cargo test --locked --workspace -- --test-threads=1
cargo clippy --locked --workspace --all-targets -- -D warnings
npm run build:desktop-test
# Windows:
node tests/desktop/run.mjs --plugins-editor
# Ubuntu, GDK_BACKEND=x11:
xvfb-run -a -s '-screen 0 1280x800x24 -nolisten tcp' dbus-run-session -- node tests/desktop/run.mjs --plugins-editor
```

| Guest | Workflow evidence | Desktop SHA256 | CLI SHA256 |
| --- | --- | --- | --- |
| Ubuntu ARM64 | `linux-deadline-results/desktop-thcja5` | `7c0c8615b812094ab318761b3f697db85b067ed010d1f4a8663c15f7ce8a7cd4` | `e8604b6bd4628e2f401cbbbff58d736c85fdfc8321a55857295f57fa86f6544d` |
| Windows ARM64 | `windows-deadline-results/desktop-j3Is7t` | `22138c684f970a036e378c37de48dd06ec673f8854b25949274817839251c0fa` | `7f5b7b0b4408aa85c6bb1bf983ea00c49298f367ce12556e8c98997f383b5463` |

Paths are relative to `test-results/plugin-platform-20261018/`; each guest's
`*-deadline-receipt.json` and validation log record exact tools, commands and
durations. A Parallels Window-menu selection corrects the mismatched VM menu bar,
but guest clicks/keys still produce no observed response to the restart reminder;
the dedicated VM app binding times out and its toolbar action fails. No historical
report or fixed-file comparison is claimed. Historical-host comparison and
physical-platform acceptance remain open; no OS update or restart was performed.

## Complete desktop10k workflows — 2026-10-18

The full `--plugin-performance` lane passes both APIs on the five hash-pinned
plugin API/CLI fixtures: dense tagged SRT, ASS, SSA, styled WebVTT and timed WebVTT.
Every combination visits all200 finding pages and200 change pages, checking each
cue identity, raw before/after text and timing. Five complete checker/fix/accept/
Undo/Redo/save cycles verify the full read-only snapshot, every accepted cue and
format property, exact output bytes, failed-save preservation and unchanged source.
Non-WebVTT fixes combine text edits with a10ms shift; WebVTT fixes preserve embedded
absolute timestamps. Full reopen, cancellation/stale rejection and durable crash
recovery pass. Recovered state requires Save As and has no restored Undo history.
The run ends with normal code0 shutdown. No media is loaded in this scale lane.

Budgets were recorded before measurement: all ordinary measured interactions and
start feedback below1000ms; complete asynchronous check/fix preparation below10s.
The helper's1000ms deadline, pure/API/CLI500ms p95/192MiB gates, fixtures and cycle
counts are unchanged. Results are maxima, not p95 values:

| Fixture | Modern interaction / preparation ms | VSS interaction / preparation ms |
| --- | ---: | ---: |
| Dense tagged SRT | 708 / 1034 | 707 / 1026 |
| ASS | 371 / 985 | 419 / 1304 |
| SSA | 379 / 984 | 374 / 1159 |
| Styled WebVTT | 413 / 1410 | 418 / 1554 |
| Timed WebVTT | 386 / 1079 | 384 / 1422 |

Across4000 review pages and50 ordinary cycles, the largest interaction is708ms
and largest complete preparation1554ms. Unix `ps` sampling records4955 samples,
699 observing a direct helper, and never observes multiple admitted helpers.
Peak combined app/direct-helper RSS is350,912,512 bytes (334.65625MiB). This is a
descriptive desktop measurement including accepted state/history and excludes
WebKit/native subprocesses; it is not the separate pure/API/CLI memory budget or
a hard memory cap. Sampling may miss transient peaks.

Reproduce with the same pinned tools, isolated app/CLI hashes and private runtime
as the native workflow above, after generating the established fixtures:

```sh
python3 scripts/ass-fixtures.py
python3 scripts/webvtt-fixtures.py
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  node tests/desktop/run.mjs --plugin-performance \
  --binary "$PWD/test-results/plugin-native-20261018/VisualSubForgePlugins.app/Contents/MacOS/visualsubforge"
node --check tests/desktop/plugin-performance.mjs
npm run format:check
npm run lint
git diff --check
```

The final complete pass is `test-results/desktop-Eo4kNB/plugin-performance.json`,
with normal shutdown in its `receipt.json`; the log and compact summary are
`test-results/plugin-native-20261018/scale-final.log` and `scale-final-summary.json`.
All individual timings, per-phase RSS, source/output/recovery hashes and checkpoint
sizes remain in the full report. Node syntax, Prettier, ESLint and diff checks pass.
These changes affect only tests/docs; the501 Rust/44 frontend/60 browser baseline
from the controls increment remains separate evidence rather than a claimed rerun.

The first full attempt, `desktop-ZXnKqr`, passed eight combinations but failed its
timed-WebVTT expected-byte assertion. That fixture deliberately mixesCR/LF/CRLF;
the initial independent oracle assumed one newline style. Removing only the10000
appended markers from the actual saved file restores the source byte-for-byte,
including every mixed separator. `scale-first-oracle-diagnosis.json` records that
proof. The corrected oracle retains each separator and requires exactly10000
payload changes; the final run repeats all ten combinations. No product code,
execution limit or input was changed to pass. Historical-host and available
Windows/Linux checks still keep stage7 and ADR0008 open.
