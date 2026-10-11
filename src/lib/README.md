# Frontend ownership

`App.svelte` composes the workspace and owns DOM layout, focus, modal presentation,
view options, and transient gestures. Its Svelte session objects live for one
mount and are disposed on unmount; they are not global stores.

| Owner | State and responsibility |
| --- | --- |
| `DocumentSession` | Replacement-only projection of Rust's document, selected cue, and text/time drafts. It permits one document command at a time and reconciles selection/drafts only after acceptance. |
| `MediaSession` | Projection of native playback, response ordering, polling timer, and `LiveLoop`. New commands invalidate older status responses. Replacement and loop clearing drain pending loop commands first. |
| `ProjectWorkflow` | Projection of Rust's project attachment, ordered project open/save, and quit sequencing. It asks App to present decisions and update view options. |
| `client.ts` | Typed IPC functions and DTOs. All application communication passes through this module. |

The `.svelte.ts` extension lets each owner use Svelte runes directly. Accepted
snapshots use `$state.raw`: replace them rather than mutating cue objects. App's
`$derived` aliases are read-only views. Draft fields remain locally editable;
Apply sends a captured generation/revision to Rust, which owns validation and
undo history. A failed command preserves the draft. Timing gestures deliberately
send the version captured when the gesture began.

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

The browser workflow fixtures mount the actual App. `workspace-lifecycle` also
exercises these compiled Svelte owners with controlled replies, checking ordering,
rejection and disposal at each asynchronous boundary. These checks model IPC and
do not establish native GPU/audio behavior.
