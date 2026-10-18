# Frontend ownership

`App.svelte` composes the workspace and owns DOM layout, focus, modal presentation,
view options, and transient gestures. Its Svelte session objects live for one
mount and are disposed on unmount; they are not global stores.

| Owner | State and responsibility |
| --- | --- |
| `DocumentSession` | Replacement-only projection of Rust's document, immutable selected-ID set/primary cue/range anchor, and text/time/style drafts. It permits one document command at a time and reconciles selection/drafts only after acceptance. |
| `MediaSession` | Projection of native playback, response ordering, polling timer, and `LiveLoop`. New commands invalidate older status responses. Replacement and loop clearing drain pending loop commands first. |
| `ProjectWorkflow` | Projection of Rust's project attachment, ordered project open/save, and quit sequencing. It asks App to present decisions and update view options. |
| `ResyncSession` | Dialog-lived proposal projection, source selection and request ordering. It cancels late starts and discards obsolete review replies. Rust keeps the proposal; acceptance goes through DocumentSession's serialized update path. |
| `QualitySession` | Bounded report/metric projection, filters, pause state and latest draft input. One in-flight read and context guards prevent polling queues and stale publication; Rust owns evaluation and accepted findings. |
| `GapSession` | Dialog-lived proposal pages, acknowledgment and asynchronous response ordering. Rust owns calculation and acceptance; DocumentSession serializes the accepted reply with ordinary edits. |
| `client.ts` | Typed IPC functions and DTOs. All application communication passes through this module. |

The `.svelte.ts` extension lets each owner use Svelte runes directly. Accepted
snapshots use `$state.raw`: replace them rather than mutating cue objects. App's
`$derived` aliases are read-only views. Draft fields remain locally editable;
Apply sends a captured generation/revision to Rust, which owns validation and
undo history. A failed command preserves the draft. Timing gestures deliberately
send the version captured when the gesture began.

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

Disposal invalidates project continuations, stops polling, and discards queued loop
previews. App disposes the shared `PendingDecision` owners, settling both modal decisions as cancellation. In-flight IPC can
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
string errors and null success values. They do not prove native dispatch.

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

Command wrappers retain rejected promises for Rust errors and adapt JSON null
to void where appropriate. Generator annotations do not replace validation,
revision guards or cancellation. The generator's runtime helpers are excluded
from handwritten-code lint rules but remain under strict TypeScript and drift
checks. The generator is a pinned prerelease: review generated diffs, actual
Serde samples and browser/native regressions on upgrades. The optional schema
feature in media-analysis is enabled only by the desktop; headless callers do
not depend on Specta, Tauri or native playback.

## Minimum-gap review

`GapDialog` resolves drafts before `GapSession.start()` captures the current
DocumentSession projection and media session. The generated client admits one
Rust proposal worker, then polls pages. The frontend never computes repair
endpoints or treats draft text as accepted. Page/start epochs reject stale
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
`TimingRepairDialog`. Their session wrappers share `TimingRepairSession` for
draft handling, paging, disposal and late-admission cancellation. They display
immutable Rust proposals; no timing arithmetic is repeated in the frontend.
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
