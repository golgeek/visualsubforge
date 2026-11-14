# Frontend ownership

`App.svelte` is the workspace composition root. It constructs sessions, wires
effect callbacks and delegates actions through a readable switch to
`workspace-file-actions.ts`, `media-actions.ts`, `cue-actions.ts` and `view-actions.ts`. `Workspace`
owns `ready`, `error`, `notice` and its task counter; it derives `busy`/`confirming`
from modal, decision and gesture activity. App creates owners and lays out regions. Its Svelte session objects live for one
mount and are disposed on unmount; they are not global stores.

Owners with state, reactive projections or a lifetime are classes. Stateless actions,
calculations and project-view adapters are plain functions. Effect-bearing classes
are constructed during component initialization. `disposeWithComponent` registers
cleanup beside each disposable owner's construction; pollers start on mount.
Construction follows dependencies, then explicit connect calls complete the gesture,
navigation and workspace graph before effects or input run. UnsavedChanges receives
a save continuation per call, so it does not depend on ProjectWorkflow.

Cleanup invalidates guards synchronously in construction order. Media stops its
live-loop owner before gesture/timeline teardown. Decision settlement queues promise
continuations; those run only after all owners have been disposed. `captureFocus`
supplies the identical return-focus policy for modals and cue menus.

Admission differences are preserved: row/continuation selection checks busy, while
problem navigation and select-all additionally check confirming. Legacy opening is
already task-gated and its confirmation projection uses visible/closing; other
decisions expose their full active lifecycle. These are existing entry-point rules,
not a second workspace gate.

## Browse by area

| Folder | What to look for |
| --- | --- |
| `workspace/` | Workspace gate, startup, actions, keyboard routing, menu subscription and shell controls. |
| `cues/` | Selection, editing, navigation, cue menu, table and subtitle controls. |
| `document/` | Accepted document projection, drafts, unsaved-work guard and recovery. |
| `timeline/` | Waveform viewport/navigation and scene controls/state. |
| `timing/` | Held timing, magnets, cue endpoints and reviewed timing repairs/resynchronization. |
| `media/` | Playback session/actions, live loop, native layout, video, transport and diagnostics. |
| `dialogs/` | Shared modal/decision presentation, focus capture and workspace dialog rendering. |
| `project/` | Project open/save/restore, reference/save decisions and legacy import. |
| `quality/` | Checks, metrics, findings, language notices and statistics. |
| `authoring/` | Text, styles, fonts, conversion and plugin review sessions/dialogs. |
| `settings/` | Preferences and quality/timing settings. |
| `translation/` | Reference session, file operations and panel. |
| `layout/` | Pane geometry, measurement and splitters. |
| `ui/` | Theme tokens, element defaults and the canvas palette read from tokens. |

Pure unit tests live beside the code they exercise. `client.ts` remains the single
IPC facade; `generated/` contains generated contracts. The owner map below explains
responsibilities across these folders.

| Owner | State and responsibility |
| --- | --- |
| `Workspace` | Workspace task counter, readiness, error and notice; derives busy and confirming from owner activity and exposes the shared admission check. |
| `CueNavigation` | Explicit selection, keyboard focus, pending problem navigation, reveal and cue-following effects; narrow DOM callbacks and mount-lifetime guards. |
| `WorkspaceView` / `WorkspaceLayout` | Panel visibility/rules and measured pane sizes/splitter constraints, respectively. |
| `WorkspacePreferences` | Saved defaults, effective project timing, metric visibility and quality-context invalidation, updated through initialization, acceptance and restore methods. |
| `WorkspaceScenes` | Scene visibility, cuts, controls, status and restored-generation projection. |
| `UnsavedChanges` | Subtitle save and the unsaved-changes guard; each caller supplies its project-save continuation. |
| `TranslationReference` | Protected reference attach/copy/offset/detach actions and lifetime guards. |
| `WorkspaceStartup` / `WorkspaceMenus` | Initial state loading and native menu projection/subscription, respectively; each guards late replies after disposal. |
| `NativeVideoLayout` | Coalesced native layout requests, resize observer, window listeners and polling timer; disposal removes all subscriptions and suppresses late replies. |
| `DocumentSession` | Replacement-only projection of Rust's document, immutable selected-ID set/primary cue/range anchor, and text/time/style drafts. It permits one document command at a time and reconciles accepted replies and authoritative refreshes while retaining drafts. |
| `TimingGesture` | Held-Space timing, boundary preview/captured version, magnetic targets and range/loop restoration. It exposes activity; Workspace derives the gate and owns messages. |
| `CueTableView` | Fixed-density virtual rows, original-index-preserving filters, column projection and active cue IDs, read from accepted subtitle and playback callbacks. Synchronous setters ignore updates after disposal. |
| `TimelineView` | Waveform viewport, selection range and live-loop preview updates, visible cues, duration, analysis start/polling and stale-reply rejection. App supplies playback and gesture state. |
| `MediaSession` | Projection of native playback, response ordering, polling timer, and `LiveLoop`. New commands invalidate older status responses. Replacement and loop clearing drain pending loop commands first. |
| `WorkspaceDecision` | Unsaved-work, project-save-failure, project-reference and legacy-import hide/wait/answer lifetimes. Decisions own typed display payloads; LegacyImportDecision also owns return focus. App supplies media callbacks; Workspace derives confirming; save/exit policy stays in UnsavedChanges and ProjectWorkflow. |
| `HideVideo` | Shared hide-before-show, render-before-restore, remaining-occlusion, epoch and disposal lifecycle for modals, decisions and the cue menu. |
| `CueMenu` | Captured cue/range context, menu replacement, command activity and menu focus lifetime. |
| `Recovery` | Recovery listing, polling/response epochs, discard confirmation and recovery action activity. |
| `WorkspaceDialogsState` | Dialog eligibility, inputs and post-close continuations over WorkspaceModal identity; accepted content remains with DocumentSession. |
| `WorkspaceModal` | Authoring, timing, Settings, resync, statistics and text-dialog identity/transitions, native-video occlusion handshake and focus restoration. App supplies effects/eligibility; feature sessions retain drafts and acceptance. |
| `ProjectWorkflow` | Projection of Rust's project attachment, ordered project open/save, and quit sequencing. It asks App to present decisions and update view options. |
| `TranslationSession` | Reference query pages, mutation/query lifetime and latest desired selection. It cancels/drains obsolete work and delegates accepted context/target copies to DocumentSession. |
| `LegacyImportSession` | Dialog-lived file selections, immutable review projection and reply epochs. Rust inspects and publishes a new project; only an explicit Open action enters ProjectWorkflow's protected restoration. |
| `ResyncSession` | Dialog-lived proposal projection, source selection and request ordering. It cancels late starts and discards obsolete review replies. Rust keeps the proposal; acceptance goes through DocumentSession's serialized update path. |
| `QualitySession` | Bounded report/metric projection, filters, pause state and latest draft input. One in-flight read and context guards prevent polling queues and stale publication; Rust owns evaluation and accepted findings. |
| `TimingRepairSession` | Shared gap/scene/duration/batch review requests, bounded pages, context guards and disposal. Rust owns calculation and acceptance; DocumentSession serializes accepted replies with ordinary edits. |
| `GapSession` | Gap-specific client adapter and no-media acknowledgment/eligibility over the shared review lifetime. |
| `client.ts` | Typed IPC functions and DTOs. All application communication passes through this module. |

Decision payloads are published only after native video hides and are cleared before
restoring remaining occlusion and settling the answer. LegacyImportDecision keeps
publication/open continuation and return focus together; no external open-flag shim
is needed. UnsavedChanges does not depend on ProjectWorkflow: callers explicitly
supply the appropriate save continuation, including the project operation's own
save context. TranslationSession clears hidden reference queries in its initialization
scoped effect; TranslationReference handles user file operations and quality invalidation.

Panel rules preserve existing distinctions: explicitly closing Problems cancels its
pending navigation, toggling Translation only hides Problems, and Statistics can
reveal Problems while retaining Translation visibility. Preference initialization,
Settings acceptance and project timing restoration each update the related fields
through one owner method; project timing never replaces saved defaults.

`CueTable`, `CueEditor` and `WaveformPanel` receive the relevant session owners.
They share the `Workspace` object with gestures, modals and decisions. Its
`canAct` read combines readiness, busy and confirming; callers retain their
feature eligibility checks. `task()` counts pending tasks through completion,
reports failures and decrements in finally. Nested and overlapping tasks keep busy
until the last task finishes. Callers that previously retained
an error opt out of clearing it; a failed task returns false to stop a continuation.
Feature owners expose activity and never assign the workspace gate. Components
keep their scoped styles and DOM bindings;
CueNavigation owns selection, problem navigation, following and focus/reveal
sequencing through viewport/scroll/focus callbacks. It uses workspace tasks and never assigns the gate. `CueTableView` is synchronous, so it
needs no request epoch; CueTable disconnects its resize observer on unmount. `pane-layout.ts` computes measured minima and splitter bounds
without DOM access. The shared measurement action schedules observer updates on
the next animation frame and cancels that callback during cleanup.

`workspace-keys.ts` contains shortcut routing and editing-key execution. The
pure router is tested without DOM or IPC; its adapter classifies focused text
fields and controls, consumes handled keys and delegates to the owners. Text
history remains local for native menu/keyboard Undo/Redo; explicit history buttons
still use document history. Action modules retain scene-action exceptions to the
task gate and read live state after asynchronous draft/file operations. WorkspaceView owns panel toggles and their rules, and WorkspaceLayout owns measured pane sizes
and splitter constraints. UnsavedChanges and project-view retain the protected save/restore sequence;
TranslationReference owns reference file operations next to TranslationSession. CueEditing owns commit triggers and focus identity; WorkspaceQuality derives quality inputs from live owners; WorkspaceMenus owns menu projection and subscription.
WorkspaceStartup loads initial state; NativeVideoLayout tracks the video element.
Its disposal disconnects the observer, removes window listeners and clears the
100 ms timer; in-flight replies cannot publish after disposal.

The shell components own toolbar, recovery-banner, footer, diagnostics and font
notice markup and styles. TransportBar, CueOperationsBar and SubtitleToolbar own
the subtitle controls; five review buttons share one list and disabled condition.
VideoPreview and ActionBrowser own their markup and styles. WorkspaceDecisions
and WorkspaceLegacyImport render decisions, retaining their DOM order around
WorkspaceDialogs and the native dialog binding used by UnsavedChanges. WorkspaceDialogs renders feature dialogs from the
modal identity; WorkspaceDialogsState owns their eligibility and post-close
continuations. Recovery owns one polling timer, rejects stale listings around
discard and stops polling on disposal. Recovery and CueMenu expose action
activity for Workspace to derive busy; neither assigns its gate.

HideVideo supplies the common hide/render/restore and epoch/disposal guards.
WorkspaceModal retains focus capture and reported open failures; WorkspaceDecision
retains thrown open failures and answer settlement. CueMenu retains replacement
of an open menu, captured command context and immediate return focus. The shared
lifecycle restores remaining occlusion after rendering removed content.

The `.svelte.ts` extension lets each owner use Svelte runes directly. Owners
that use `$effect`, including `TimelineView` and `TimingGesture`, must be constructed during component
initialization so Svelte owns their effects. Start polling on mount and call
`dispose()` on unmount; alive/request guards suppress late replies. Timeline
reset clears analysis and range together; project restoration calls `restore()`
to apply its saved viewport with the existing media bounds. Accepted
snapshots use `$state.raw`: replace them rather than mutating cue objects. App's
`$derived` aliases are read-only views. Draft fields remain locally editable;
CueEditing schedules a 700 ms pause, ordinary blur or explicit flush, with IME,
modal/decision admission and disposal guards. Rust coalesces only eligible
same-focus text commits; the frontend owns no accepted history. Explicit cue
commands drain a pending automatic commit before committing newer fields.
Stale/unknown failures refresh once without replay, preserving the newest draft.
A failed refresh blocks mutation; retry first reads authoritative state and asks
for review. Known rejection retains an inline error. Escape waits for a dispatched
commit, restores confirmed accepted fields and returns focus to the waveform;
it cannot discard an unknown outcome. Modal/decision focus changes retain their
existing draft protection and restart the pause delay on return. Timing gestures deliberately
send the version captured when the gesture began. `TimingGesture.active` covers
boundary previews and held-cue commits; Workspace includes it in `busy`. A held Space
preview remains nonblocking, so playback following continues. Disposal suppresses
late messages and loop restoration, while already accepted Rust edits remain accepted.

ASS/SSA snapshots include read-only styles/script settings and their 10 ms time
grid. `StyleInspector` owns only its inspected style selection; assignment belongs
to DocumentSession's draft and commits with text/time in one `editStyled` command.
Unknown existing style names stay visible. Opening the inspector neither applies
nor discards a draft. Playhead/gesture operations quantize before commit; typed
values remain exact and Rust rejects unrepresentable endpoints. Formatting sends
document identity plus the draft style so Rust can resolve the inherited property;
App rejects a reply if document, cue, text or style changed while it was pending.
See the [ASS/SSA integration guide](../../docs/ass-ssa-documents.md) for current
validation scope and format-aware repair policies.

Project references validate before the save/discard decision. Starting replacement
clears the frontend attachment; only a complete media/subtitle/scene/seek restore
adopts the new token. Later failures leave a usable, unattached workspace instead
of making quit overwrite the previous project. Ordinary media open/close detaches
the project explicitly. Project save failures during quit retain the separate
subtitle decision and offer retry, Save As, cancellation, or explicit exit.

The four workspace decisions share `WorkspaceDecision`: it hides native
video before presenting content, removes content before restoring diagnostics
occlusion, and returns cancellation on disposal or restore failure. Hide failures
still propagate to the requesting action. `protect()` and ProjectWorkflow retain
their original save/exit continuations. Legacy import uses the same presentation/answer owner but releases the action
gate after opening and retains confirming through restoration. Its publication
owner and explicit protected project-open continuation remain separate. CueMenu keeps its selection/navigation and focus lifetime over HideVideo.

Translation reference issues are resolved before unsaved-work protection. Missing
references permit relocation; changed contents require explicit reload. WorkspaceDecision owns
the presentation/occlusion ordering; App supplies dialog content and media callbacks. ProjectWorkflow retries
preflight with the chosen override; Rust owns the captured reference and its
identity. If protecting current work saves its project, the next project is
preflighted again because publication invalidates pending attachment tokens.
After attachment/save, a document status reply refreshes accepted context.
Context-only dirty changes prompt for project save before replacement/quit;
discarding them on quit skips automatic project publication. Subtitle-only
projects skip media loading, scenes and seeking, including in playback-free builds.

TranslationPanel owns browse/match mode, page and selected reference row. Its
inputs follow accepted target timing and visibly label unapplied drafts. App
gates reference changes and protects independent target-copy replacement.
TranslationSession serializes query work, coalesces desired selection, cancels
late job identities and drains obsolete work before mutation. Hidden/disposed
panels cancel reading; accepted references remain Rust-owned. Reference acceptance
uses DocumentSession's existing serialization: context-only replies preserve text,
timing/style drafts, while a new target generation reconciles normally. A status
refresh after a failed acceptance reply accounts for a potentially successful
Rust commit without retrying the change. Operation errors survive background
query resumption. The panel never executes source HTML, CSS or ASS markup.

Legacy import is independent of the active document/media projection. Relocation
retires old reviews; choosing a different project clears earlier relocations.
One status request runs at a time. Closing cancels known/late jobs, while a picker
or unresolved publication keeps the modal alive. Successful publication remains
visible if it wins a cancellation race. Creating the output never enters document
adoption or project attachment; Open created project preflights the exact output
path and then uses the ordinary unsaved-work decision. See the
[import contract](../../docs/legacy-project-import.md) for backend ownership and
remaining native/scale acceptance.

Disposal invalidates project continuations, stops polling, and discards queued loop
previews. App disposes the shared `WorkspaceDecision` owners, settling pending modal decisions as cancellation. In-flight IPC can
still complete in Rust; disposal does not roll back an accepted operation or
pretend to cancel native IO. Late replies cannot update the disposed projection
or dispatch the next project/Save As step.

Resync captures the accepted snapshot after an explicit cue-draft apply/discard
choice. The modal keeps native video occluded; it cannot close during a document
mutation acknowledgment, so a late draft/accept reply cannot overwrite new typing
behind it. Closing during analysis cancels the known job; closing during Start
cancels the job when its delayed ID arrives. Input/capture changes retire review,
and only a complete proposal enables acceptance. Rust remains authoritative for
all final identity, history and source-file protection checks.

The browser workflow fixtures mount the actual App. `workspace-lifecycle` also
exercises these compiled Svelte owners with controlled replies, checking ordering,
rejection and disposal at each asynchronous boundary. These checks model IPC and
do not establish native GPU/audio behavior.

## Quality profile Settings

Settings holds a deep, dialog-local draft of the accepted quality profile and
timing settings fetched from Rust. Its captured policy epoch guards Apply;
failed persistence and stale drafts cannot change accepted rules. Import changes
only this draft, export writes it explicitly, and Cancel discards it. The pure
`quality-profile.ts` helpers prepare custom drafts; they do not evaluate quality
or own accepted state. Built-in values come from Rust-generated
`generated/quality-defaults.ts`, avoiding a second handwritten rule definition.
ProjectWorkflow restores timing/view projections after Rust accepts the staged
project profile; the profile itself remains Rust-owned and is captured there on
project save. See the [quality guide](../../docs/subtitle-quality.md).

## Quality checking

`QualitySession` debounces changed inputs, fetches at most 100 Problems findings
(or 200 index findings) and 200 metric rows per request and cancels late worker admissions when closed/disposed. It never evaluates
rules in TypeScript. App supplies visible rows, selection and the unapplied cue
draft; accepted and draft metrics remain visibly separate. Profile epochs force
unchanged drafts to be resubmitted after Rust invalidates them. F4 and Problems
buttons use the same protected Apply/Discard/Cancel navigation before calling
existing cue selection. Failed Apply cannot discard a draft or move selection.

Rust captures immutable domain/profile inputs under the document mutex, then
evaluates outside it on one CPU worker. No queued document snapshots accumulate.
One latest draft replaces older draft work, with accepted checks taking priority.
Generation, revision and policy epoch guard publication and cancellation; running
work is cancelled on invalidation and reaped before replacement. Shutdown joins
outside the document lock. Read projections bound individual line-length arrays
to 20 while preserving full line counts/maximum lengths. The full shared report
stays in Rust and never travels through ordinary polling IPC.
Media session and scene generation/readiness also guard results. App supplies a
scene-context key independently of viewport changes; Rust captures the actual
completed cuts. A rescan cancels obsolete checks even without a document edit.
Missing, partial and completed heuristic coverage remain distinct in Problems.

## Generated IPC

All commands, the workspace-action event and their JSON contracts are generated
from Rust through exactly pinned `tauri-specta`/`specta` 2.0.0-rc.25 and
`specta-typescript` 0.0.12. `src-tauri/src/ipc.rs` is the single command/event
registry used by desktop dispatch and generation. `client.ts` supplies short
type aliases, browser defaults and diagnostic hooks; components and session
owners keep using that facade. No protobuf or separate Rust service is involved.

`npm run ipc:generate` invokes the explicit, ignored `ipc::export_bindings`
exporter test, formats its output with pinned Prettier and writes
`generated/ipc.ts` and built-in profile defaults. Normal tests and application
startup never rewrite bindings.
`npm run ipc:check` compares a temporary export with the checked-in files; the
desktop CI matrix runs it. The exporter also serializes actual Rust presets,
document/recovery snapshots, opened projects, scene/waveform views, preferences,
media status and event payloads into `tests/frontend/ipc-contract-samples.ts`.
Strict TypeScript checks these samples against the generated declarations.
Browser workflows exercise the facade with controlled responses, including
tagged document errors, other commands’ string errors and null success values. They do not prove native dispatch.

Generated names include Rust module paths; the facade chooses deserialize shapes
for inputs and serialize shapes for outputs. Old project inputs may omit timing
settings; Rust defaults them, and opened projects always include them. Project
fields remain flattened beside the token. The compatibility trial found a
pinned-generator phase-splitting collision even with module-qualified names:
two codec-bearing structs named View produced the same body. Their Rust DTOs now
have distinct names, SceneView and WaveformView. Actual Serde samples guard these
shapes. Avoid container Serde renames for cosmetic exported names; the facade
provides aliases without changing wire data. No generator patches are carried.

Wide IDs, revisions, sizes, counters and millisecond values remain Rust u64
internally. At the JSON boundary, `ipc_number::SafeU64` and field codecs permit
only integers from 0 through 9,007,199,254,740,991. Invalid input and
unrepresentable output fail; there is no global lossy-number override or JSON
bigint claim. Optional recovery metadata outside that range is unavailable
(null) in listings; stored records remain recoverable with fresh editor history.
Required floating-point output uses finite-number serializers, since ordinary
Serde would turn NaN/infinity into null. Optional native properties retain their
number-or-null contract. Validation tests cover the numeric codecs and output
collections. Generation never changes the underlying domain types.

The document command rejects with a generated tagged error; the facade wraps its
kind/message in `DocumentFailure` without parsing display text. Stale revisions and
replaced generations are classified under the existing document mutex before
mutation. Worker/transport failures can have unknown outcomes. Other command
wrappers retain their existing rejected promises for Rust errors and adapt JSON null
to void where appropriate. Generator annotations do not replace validation,
revision guards or cancellation. The generator's runtime helpers are excluded
from handwritten-code lint rules but remain under strict TypeScript and drift
checks. The generator is a pinned prerelease: review generated diffs, actual
Serde samples and browser/native regressions on upgrades. The optional schema
feature in media-analysis is enabled only by the desktop; headless callers do
not depend on Specta, Tauri or native playback.

## Minimum-gap review

`workspace/ReviewDock` places Issues, Fix timing, Shift and Statistics beside the
cue table without overlapping native video. Its scope follows accepted list IDs
or the active filter. `timing/ReviewTiming` presents gap, scene, duration and batch
sessions; it owns only option/exclusion controls and bounded projection callbacks.
`GapSession` retains coverage policy and delegates to `TimingRepairSession`.
Live inputs debounce 250 ms; semantic input/viewport keys avoid starvation from
unrelated background updates. Stale plans retire before new admission. Read-only
admission failures stop after three attempts, with explicit Retry; acceptance is
never retried. Closing during admission cancels a late returned job identity.

Rust prepares selected repairs through `quality::timing_selection`, re-evaluates
the whole candidate and reports unsafe coupled subsets without accepting them.
The frontend never recomputes repair endpoints. Pages contain at most 100 rows;
ghost/list projections request at most 200 visible IDs, independently of paging.
`review-preview.ts` formats supplied endpoints only. The UI labels the projection
limit; Apply includes the entire checked subset as one history operation.
Acceptance contributes `WorkspaceView.reviewBusy` to workspace admission until
DocumentSession reconciles its reply. Other controls and tab closure are disabled
during that acknowledgment. The explicit no-media note replaces the gap checkbox;
attached-media scene requirements remain enforced. Successful acceptance restarts
quality checking and leaves the dock available for the next proposal.

The backend holds media selection → scene state → document during acceptance.
It checks the captured scene generation/readiness, media session and document
revision/profile epoch before applying the immutable plan as one history entry.
All capture/projection/commit commands run on blocking workers. Evaluation owns
an immutable clone outside those locks; shutdown cancels and joins outside them.

## Scene and duration timing review

Scene and duration wrappers retain their coverage eligibility while sharing the
live session owner. Changing scope, exclusions or context immediately invalidates
acceptance. App routes timing tools only through ReviewDock; the obsolete modal components
and identities are removed.
DocumentSession serializes acceptance with other document actions. Completed
previews continue polling for backend profile/scene invalidation.

Shared Rust scene proposals trim inward; duration proposals keep starts fixed
and choose the nearest safe end. Both preserve text/tags and verify the complete
candidate against timing constraints. The desktop shares one timing-proposal
worker, global job IDs and purpose guards, with no queue and a ten-second deadline.
Media → scenes → document locks
guard capture and acceptance; evaluation and shutdown joins happen outside the
document lock. The existing `Edit::Retime` supplies one atomic Undo/Redo entry.
See the [scene contract](../../docs/scene-quality.md) and
[duration policy](../../docs/duration-repairs.md) for skips and coverage.

## Document statistics

`StatisticsPanel` starts a read-only report without resolving or modifying a cue
draft. `StatisticsSession` captures the accepted revision, media/context and scope;
one in-flight poll projects the summary and 100-row outlier pages. Draft typing
leaves the report valid, while document/profile/media/scenes or scope changes
hide old results. Reactive capture and request epochs reject late responses;
closing also cancels a late worker admission. Current cue links route through
existing Problems Apply/Discard/Cancel navigation. The dock stays outside native
video bounds and restores its launcher focus on close.

The typed client calls three thin worker-dispatched commands. Rust's dedicated
statistics owner clones accepted inputs under media → scenes → document ordering,
then evaluates shared quality metrics and aggregates outside locks. It retains
only summary/findings, with one worker, no queue and a ten-second deadline. Reads
validate captured keys; shutdown joins outside the document lock. This fifth
bounded CPU service avoids interrupting automatic checks or repair reviews.
The CLI calls the same pure report engine. There is no statistics acceptance or
second document/history owner. See [statistics](../../docs/statistics.md) for
denominators, boundaries and validation.

## Text tools

`TextDialog` resolves a cue draft before `TextSession` captures its accepted
document projection and search/repair inputs. The typed client starts a Rust
worker and polls bounded cue/match pages. Changes to inputs immediately disable
acceptance; document/profile changes retire the Rust proposal. Closing while
admission is pending cancels the eventual job ID. `DocumentSession.acceptText()`
uses the same serialized adoption path as ordinary edits, resync and gap repair.
The dialog cannot close during acceptance; accepted edits restart visible checks.

Rust owns the original source and immutable plan. The shared operation maps
search text back to source offsets and preserves protected tag/escape tokens.
`Edit::TextBatch` changes only cue text and validates the complete document before
publishing one history operation. Frontend code never reconstructs a batch from
stripped text or sends proposed replacement cue arrays back to Rust.

Formatting flushes the current fields first, then captures source text, cue
identity, accepted snapshot and UTF-16 selection. `textFormat` transforms it in
shared Rust code; a reply must still match the captured inputs before a separate
ungrouped commit. Returned selection offsets are restored after acceptance.
See [the text tool contract](../../docs/text-editing.md) for supported syntax,
conservative skips, limits and evidence.


## Batch timing and selection

`cue-selection.ts` implements file-order ranges, disjoint toggles and stable-ID
reconciliation as pure transformations. `DocumentSession` replaces these immutable
sets along with its primary cue after protected selection and accepted commands.
`CueNavigation` owns keyboard focus and virtual scrolling; focus-only Ctrl/Cmd navigation
does not change membership. Existing single-cue controls still edit the primary.

`ReviewTiming` uses the batch adapter of `TimingRepairSession` and
captures an explicit request. Semantic comparison handles derived-object
recomputation without accepting changed inputs. The paused playhead is captured,
not followed during review. Rust computes exact endpoints; `acceptBatchTiming`
uses the ordinary document replacement path. See [batch timing](../../docs/batch-timing.md)
for rounding, cancellation, worker ownership and measured limits.


Font attachment review follows the style owner: `FontDialog`/`FontSession` keeps
only inventory metadata, selection and review identity. Native picker completion
rechecks the captured document; draft edits block proposals and acceptance.
`DocumentSession.acceptFont` adopts the sole Rust-owned result. The four typed
font commands reuse the text/style worker, including inventory and bounded font
file reads. Reviewed bytes are captured in Rust and not reread on acceptance.
See style-authoring.md for source/history ownership, bounds and validation.

`ConversionDialog`/`ConversionSession` reuses the authoring modal lifecycle but
does not call a document adoption method. Review captures the accepted projection
and options; draft, snapshot or option changes immediately block export. The typed
client dispatches preparation and polls a bounded Rust report. A successful
review enables a format-specific new-file picker. Picker completion rechecks the
capture, then publication reuses the Rust review worker. The frontend waits for
the terminal receipt even if the dispatch reply is lost; a late successful receipt
is retained despite cancellation or a changed document. Closing is disabled during
the picker/publication, and unmount cancels an eventual late job ID. Export never
changes accepted state, saved identity or history. See
[conversion](../../docs/format-conversion.md) for policies and validation scope.

`PluginDialog`/`PluginSession` keeps explicit package/scope/preferences/settings
drafts and bounded report projections. Rust's plugin owner retains the proposal.
The session serializes page/chunk reads, invalidates old epochs, cancels late starts,
and compares the current accepted snapshot plus transient invocation context.
`DocumentSession.acceptPlugin` shares the ordinary snapshot/history adoption path.
A failed acceptance reply triggers an authoritative status refresh, preserving
drafts if the accepted version is unchanged. Failed refresh locks continuation
until an explicit retry succeeds. App composes `WorkspaceModal` for the authoring
gate, native-video occlusion and focus restoration. Styles, fonts, conversion and
plugins share this mount-scoped presentation owner; their sessions keep
acceptance/draft rules. See [desktop plugins](../../docs/plugin-desktop.md) for
options, complete-text access and validation boundaries.

Validation commands and their scope are in the [test guide](../../tests/desktop/README.md).
Record exact run results and evidence in the PR description, not this ownership guide.

## Action discovery and tooltips

`workspace/action-descriptions.json` is the shared source of labels, legacy
aliases and shortcuts. Rust embeds it for native menu labels/accelerators;
`action-registry.ts` supplies typed discovery and availability reasons.
`workspace-keys.ts` and `timing/shortcuts.ts` retain pure routing and existing
focus/AltGr/composition rules while reading registry bindings. Native menus keep
their platform placement and existing `WorkspaceMenus` eligibility projection.

`WorkspaceCommands` coordinates existing owners after closing discovery, checks
current eligibility again and rejects cue jumps if the document changed during
close. It owns no draft, transaction, timer or subscription. `ActionBrowser`
keeps only query/focus/pending interaction state; palette, Commands menu and
shortcut sheet use the existing `WorkspaceModal` lifetime. Escape restores focus
and remaining native-video occlusion before another command can start.

`Tooltips` owns a cancellable delay and one presentation through `HideVideo`.
Modal/menu admission retires hover presentation; other active operations and
overlays block new tooltips. Its serialized close prevents a newer hover from racing a pending restore;
disposal retires both delay and presentation. `Tooltip` anchors and `TooltipLayer`
render registry labels/shortcuts with tokens. Other overlays preserve tooltip
occlusion, and tooltip close preserves modal/menu/diagnostic occlusion.

Default bindings are unchanged. The existing macOS native Merge with Previous
and webview Replace both use Command+Shift+H on different routing surfaces;
this cross-surface collision remains explicit in registry tests until a shortcut
preset decision resolves it. The shortcut sheet describes native/text behavior;
it does not claim all chords are globally conflict-free.

## Layout slots and presets

`layout/layout-presets.ts` describes Timing, Editor above list, Review,
Translation and Big picture. `pane-layout.ts` computes slot minima and clamped
sizes; `WorkspaceLayout` retains independent ratios, measurements and serialized
save work. Switching CSS grid areas does not recreate feature/session owners.
The layout picker uses `WorkspaceModal`; palette presets and the editor arrow
enter the same layout owner. Reset Pane Sizes resets only the active preset.

Before changing geometry, the layout owner waits for `HideVideo`, then renders
and awaits `NativeVideoLayout.flush()` before restoring remaining occlusion.
Pointer updates coalesce while hiding; saves debounce and serialize. Disposal
cancels timers and ignores late acknowledgments. Saved sizes pass through the
single typed client, without quality-context invalidation or accepted edits.
The transport strip contains playback, range, timing, shots, snapping and compact
cue operations. Tool launchers use Commands/palette; following and metric controls
share the cue header. Snap changes the effective editing aid; saved profile defaults
remain under Settings. Per-project layout is not implemented.


## Cue columns and list filters

`cue-columns.ts` defines the presentation model, validation-compatible bounds and
pure filters. `CueTableView` owns transient query/Issues filtering and virtual-row
geometry; every row retains its original index and stable ID. The table renders
24 px compact or 32 px comfortable rows. Timing, Review, Editor above and Big picture
default to compact; Translation defaults to comfortable. Column order/width/visibility
and density save independently per preset through WorkspaceLayout. The modal chooser
uses HideVideo, and header fit measures only mounted rows to bound DOM work.

`QualitySession` uses the same serialized read lifetime for the Problems projection
and an independent accepted issue index. Only the highest Rust-provided severity per
cue is retained, including related cues. Accepted metric windows align to original
200-row ranges and evict beyond 32 windows. Report-key changes clear both projections;
late context replies cannot republish them. Incomplete/error states stay visible.
The mandatory issue column requests accepted checks even when editor metrics and
Problems are hidden. No quality rules are computed in the table.

Filtered Shift selection and Select All use visible order. Row clicks preserve
following. Programmatic reveals acknowledge their clamped offset before scroll
events; manual scrolling pauses following. Jump clears filters to reveal the active
cue and resumes unless a protected draft/editor interaction prevents it.


## Cue editor and safe fixes

`TimecodeField` shares exact typing and nominal frame nudging. The native control
worker supplies an optional container cadence; the pure nudge helper quantizes to
format precision and clamps zero. Missing cadence disables nudges, not typing.
`CueMetrics` renders Rust/profile-backed meters. The adjacent bounded source-line
counts explicitly distinguish source lines from wrapped display lines.

`CueIssueStrip` renders three chips and one fixed detail row. `QualitySession`
requests independent selected-cue issue pages, including related findings; Rust
merges speed findings and orders errors first. `CueIssuesDialog` uses
WorkspaceModal/HideVideo and 200-issue pages. No frontend quality rule is evaluated.

`CueFixes` debounces selected accepted context, serializes admission/polling,
cancels obsolete or late admissions, and retires plans when a modal owns the
workspace. It reuses Rust's bounded timing-proposal worker, not a new service.
`DocumentSession.acceptCueFixes` alone adopts the atomic result and refreshes an
unknown acceptance without replay. The application validates combined reflow,
unambiguous language and timing changes against the whole document; only the
selected cue changes, without splitting. See [cue editing](../../docs/cue-editor.md)
for interaction semantics and conservative repair limits.
