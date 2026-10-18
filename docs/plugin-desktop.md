# Desktop subtitle plugins

Stage7 remains in progress. The desktop controls compose the bounded Rust owner
and helper. Eight actual Mac format/API workflows and a separate real macOS
chooser workflow pass. Full desktop10k, historical-host comparison and available
Windows/Linux execution remain required; ADR0008 is still Proposed.

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
The helper process observation currently uses Unix `ps`; Windows needs a separate
process observation implementation before claiming this complete lane there.

Earlier failures are retained: `desktop-bm9Oob` started a review while automatic
scene analysis was still changing context, so stale protection invalidated it.
The harness now waits for completed scene context and a settled paused cursor.
`desktop-cTi2cM` passed cancellation/error controls, then hit an incorrect singular
DOM selector for two review articles. The assertion now checks both rows. No
product guard, execution deadline or fixture content was relaxed. Full desktop
scale and historical/platform acceptance remain open; these runs do not establish
physical Windows/Linux GPU/audio or release readiness.
