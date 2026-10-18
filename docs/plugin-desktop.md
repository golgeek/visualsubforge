# Desktop subtitle plugins

Stage7 remains in progress. The desktop controls and browser-tested session now
compose the existing bounded Rust owner and helper. Real desktop workflow/chooser,
full desktop10k, historical-host comparison and available Windows/Linux execution
are still required; this guide does not claim those acceptance gates.

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
