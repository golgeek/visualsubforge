# Frontend ownership

`App.svelte` is the workspace composition root. It constructs sessions, wires
effect callbacks and delegates actions through a readable switch to
`file-actions.ts`, `media-actions.ts`, `cue-actions.ts` and `view-actions.ts`. `Workspace`
owns `ready`, `error`, `notice` and its task flag; it derives `busy`/`confirming`
from modal, decision and gesture activity. App creates owners and lays out regions. Its Svelte session objects live for one
mount and are disposed on unmount; they are not global stores.

| Owner | State and responsibility |
| --- | --- |
| `Workspace` | Workspace task flag, readiness, error and notice; derives busy and confirming from owner activity and exposes the shared admission check. |
| `CueNavigation` | Explicit selection, keyboard focus, pending problem navigation, reveal and cue-following effects; narrow DOM callbacks and mount-lifetime guards. |
| `WorkspaceView` / `WorkspaceLayout` | Transient panel preferences, measured pane dimensions and splitter constraints. |
| `WorkspaceLifetime` | Startup/listener, native-layout polling and observation, and ordered owner disposal. |
| `DocumentSession` | Replacement-only projection of Rust's document, immutable selected-ID set/primary cue/range anchor, and text/time/style drafts. It permits one document command at a time and reconciles accepted replies and authoritative refreshes while retaining drafts. |
| `TimingGesture` | Held-Space timing, boundary preview/captured version, magnetic targets and range/loop restoration. It exposes activity; Workspace derives the gate and owns messages. |
| `CueTableView` | Virtual-list scroll offset/height, visible rows and active cue IDs, read from accepted subtitle and playback callbacks. Synchronous setters ignore updates after disposal. |
| `TimelineView` | Waveform viewport, selection range and live-loop preview updates, visible cues, duration, analysis start/polling and stale-reply rejection. App supplies playback and gesture state. |
| `MediaSession` | Projection of native playback, response ordering, polling timer, and `LiveLoop`. New commands invalidate older status responses. Replacement and loop clearing drain pending loop commands first. |
| `WorkspaceDecision` | Unsaved-work, project-save-failure, project-reference and legacy-import hide/wait/answer lifetimes. App supplies presentation/media callbacks; Workspace derives confirming; save/exit policy stays in file-workflow and ProjectWorkflow. |
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

`CueTable`, `CueEditor` and `WaveformPanel` receive the relevant session owners.
They share the `Workspace` object with gestures, modals and decisions. Its
`canAct` read combines readiness, busy and confirming; callers retain their
feature eligibility checks. `task()` holds the task flag through completion,
reports failures and releases it in finally. Callers that previously retained
an error opt out of clearing it; a failed task returns false to stop a continuation.
Feature owners expose activity and never assign the workspace gate. Components
keep their scoped styles and DOM bindings;
CueNavigation owns selection, problem navigation, following and focus/reveal
sequencing through viewport/scroll/focus callbacks. Its own activity contributes to
Workspace.busy; it never assigns the gate. `CueTableView` is synchronous, so it
needs no request epoch; WorkspaceLifetime disconnects its resize observer on unmount. `pane-layout.ts` computes measured minima and splitter bounds
without DOM access. The shared measurement action schedules observer updates on
the next animation frame and cancels that callback during cleanup.

`workspace-keys.ts` contains shortcut routing and editing-key execution. The
pure router is tested without DOM or IPC; its adapter classifies focused text
fields and controls, consumes handled keys and delegates to the owners. Text
history remains local for native menu/keyboard Undo/Redo; explicit history buttons
still use document history. Action modules retain scene-action exceptions to the
task gate and read live state after asynchronous draft/file operations. WorkspaceView owns panel preferences, and WorkspaceLayout owns measured pane sizes
and splitter constraints. File-workflow and project-view retain the protected
save/restore sequence. Cue-editing, workspace-quality and workspace-menus derive
projections from the live owners; all effect-bearing factories are called during
component initialization. WorkspaceLifetime owns startup, native-layout observation
and ordered disposal, preserving pending-request guards.

The shell components own toolbar, recovery-banner, footer, diagnostics and font
notice markup and styles. WorkspaceDialogs renders feature dialogs from the
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
Apply sends a captured generation/revision to Rust, which owns validation and
undo history. Stale-version and unknown-outcome cue-draft failures refresh accepted
state once, retain the draft and report a message without resending. The normal
single-window path serializes edits; there is no field merge or conflict lock.
If the target disappeared or was replaced, selection clears while text remains
available to copy. Selection and Revert keep their ordinary behavior. Failed refresh
preserves the prior projection and draft. Newer input during a successful reply
remains a draft and prevents save/navigation continuation. Timing gestures deliberately
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

`QualitySession` debounces changed inputs, fetches at most 100 findings/200 metric
rows and cancels late worker admissions when closed/disposed. It never evaluates
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

`GapDialog` resolves drafts before `GapSession.start()` captures the current
DocumentSession projection and media session. `GapSession` adapts the gap client
and explicit no-media acknowledgment to `TimingRepairSession`, shared with scene,
duration and batch review. The generated client admits one Rust proposal worker,
then polls pages. The frontend never computes repair endpoints or treats draft
text as accepted. Page/start epochs reject stale
responses; late admissions are cancelled after disposal. A completed proposal is
still polled for backend profile/scene invalidation. Acceptance blocks dialog
closure, delegates through `DocumentSession.acceptGap()` and replaces the accepted
projection only after Rust succeeds; App then restarts quality checking.

The backend holds media selection → scene state → document during acceptance.
It checks the captured scene generation/readiness, media session and document
revision/profile epoch before applying the immutable plan as one history entry.
All capture/projection/commit commands run on blocking workers. Evaluation owns
an immutable clone outside those locks; shutdown cancels and joins outside them.

## Scene and duration timing review

`SceneRepairDialog`/`DurationRepairDialog` are thin purpose wrappers around
`TimingRepairDialog`. Their session wrappers share `TimingRepairSession` with
gap/batch review for draft handling, paging, disposal and late-admission
cancellation. They display immutable Rust proposals; no timing arithmetic is
repeated in the frontend.
Duration review captures a stable selected cue ID or whole-document scope;
changing scope immediately disables acceptance. `DocumentSession` serializes
both acceptance methods with other document actions. Completed previews continue
polling so profile/scene changes disable acceptance promptly.

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

`StatisticsDialog` starts a read-only report without resolving or modifying a cue
draft. `StatisticsSession` captures the accepted revision, media/context and scope;
one in-flight poll projects the summary and 100-row outlier pages. Draft typing
leaves the report valid, while document/profile/media/scenes or scope changes
hide old results. Reactive capture and request epochs reject late responses;
closing also cancels a late worker admission. App restores native occlusion/focus
before routing a cue link through existing Problems Apply/Discard/Cancel navigation.

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

Formatting is a separate draft operation: App captures source text, cue identity,
accepted snapshot and UTF-16 selection; `textFormat` validates and transforms it
in shared Rust code. A reply only updates the still-matching draft. App restores
the returned selection and leaves acceptance to the ordinary Apply action.
See [the text tool contract](../../docs/text-editing.md) for supported syntax,
conservative skips, limits and evidence.


## Batch timing and selection

`cue-selection.ts` implements file-order ranges, disjoint toggles and stable-ID
reconciliation as pure transformations. `DocumentSession` replaces these immutable
sets along with its primary cue after protected selection and accepted commands.
`App` owns keyboard focus and virtual scrolling; focus-only Ctrl/Cmd navigation
does not change membership. Existing single-cue controls still edit the primary.

`BatchTimingDialog` shares `TimingRepairSession` with scene/duration review and
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

## Historical IPC migration receipts

These incremental counts and pending gates describe their original checkpoints.
The generated IPC contract above describes the completed migration.

Generated IPC first slice: timing preset command/DTO generation passes with
Tauri Specta/Specta 2.0.0-rc.25 and specta-typescript 0.0.12. The 70 desktop tests,
strict desktop Clippy, 39 TypeScript tests, strict Svelte/TypeScript, lint,
formatting and production build passed. All 28 Chromium/WebKit cases passed;
the timing workflow additionally checks serialized string rejection and null
success through the facade. Deterministic `npm run ipc:check` passes; a deliberate
drift control fails without rewriting the modified file. This is a first-family
trial, not completion of the remaining enum/event/project/numeric gates.

Document IPC migration: tagged edit commands, accepted snapshots, recovery and
quit now use generated bindings. Checked u64 codecs reject values outside the
JavaScript exact-integer range; a recovery listing retains usable SRTs even when
old informational metadata cannot be represented. Module-qualified generated
names avoid the pinned generator's container-rename/phase collision; the facade
keeps readable aliases. `cargo test --locked -p visualsubforge` passed 74 tests
plus the separately invoked exporter. Strict desktop Clippy, frontend build,
all 28 Chromium/WebKit cases, binding drift, lint and formatting checks passed.
Four added boundary regressions cover scalar and field codecs and recovery
preservation; generated samples cover actual Serde snapshots and all document
action variants. Other command families and project/event compatibility remain.

Full IPC migration: all 24 commands and the workspace-action event now share
one Rust dispatch/generation registry; the client facade has no handwritten DTOs
or command names. The compatibility trial found and repaired two issues: Serde
non-finite floats could become null, and two codec-bearing View structs collided
in the generator's phase output. Required numeric outputs now have checked
serializers; SceneView/WaveformView have distinct Rust names. Actual Serde
samples cover project flattening/defaults and scene/waveform/event shapes.

Validation on the development Mac: `cargo test --locked --workspace` passed 140
tests (79 desktop), strict workspace Clippy passed, and `cargo check --locked
-p visualsubforge --features media-prototype` passed. This last check is compilation,
not GPU/audio execution. `npm run build`, `npm run lint`, `npm test` (39), formatting
and `npm run ipc:check` passed. The first browser run passed 26/28: the delayed
listener controls blocked module loading after the generated runtime switched
to a static import. The controls now hold the modeled registration response;
`npm run test:browser -- --grep projects` passed all four affected Chromium/WebKit
cases, including the two formerly failing controls. `cargo tree --locked -p
vsf-cli --edges normal` confirms no Specta/Tauri/native playback dependency.
Cross-platform/native IPC execution remains part of final integration validation.
