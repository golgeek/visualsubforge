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

## Generated IPC

Timing preset, document, recovery and quit commands now generate their JSON
contracts from Rust through exactly pinned `tauri-specta`/`specta` 2.0.0-rc.25 and
`specta-typescript` 0.0.12. `client.ts` imports and re-exports these contracts;
components and session owners keep using that facade. Other command families
still have handwritten types while compatibility is evaluated. No protobuf or
separate Rust service is involved.

`npm run ipc:generate` invokes the explicit, ignored `ipc::export_bindings`
exporter test, formats its output with pinned Prettier and writes
`generated/ipc.ts`. Normal tests and application startup never rewrite bindings.
`npm run ipc:check` compares a temporary export with the checked-in files; the
desktop CI matrix runs it. The exporter also serializes actual Rust presets,
document snapshots and recovery entries into `tests/frontend/ipc-contract-samples.ts`.
The same file includes tagged command inputs accepted by Serde. Strict TypeScript
checks these samples against the generated declarations. Existing browser
workflows exercise the facade with controlled responses, including serialized
string errors and null success values.

Generated names include Rust module paths; the facade supplies short aliases
and chooses the deserialize shape for inputs and serialize shape for outputs.
This avoids collisions between common module-local names such as Action and
View. It also avoids a pinned-generator limitation found in the trial: combining
a container-level Serde rename with split input/output phases causes duplicate
export names. No dependency patch or application type renaming is needed.

Document IDs, revisions and millisecond values stay Rust u64 internally. At the
JSON boundary, `ipc_number::SafeU64` and field codecs permit only integers from
0 through 9,007,199,254,740,991. Invalid input and unrepresentable output fail;
there is no global lossy-number override or JSON bigint claim. Optional recovery
timestamps/revisions outside that range are unavailable (null) in the listing;
their stored bytes remain intact and the subtitles remain recoverable with fresh
editor history. Boundary tests cover every numeric document action field and
response category, including a recovery record with unusual metadata.

Command wrappers retain rejected promises for Rust errors and adapt JSON null
to void where appropriate. Generator annotations do not replace validation,
revision guards or cancellation. Events, flattened/defaulted project fields and
remaining command families are still compatibility gates. The generator is a
pinned prerelease: review generated diffs, contract samples and browser
regressions on upgrades. The command registration list will be unified when the
remaining families have migrated.
