# Subtitle resynchronization implementation

The first release is under development. The current alignment experiment is
documented in the [corpus guide](../tests/fixtures/resync/README.md). There is no
product CLI or desktop resync interface yet.

## Atomic retiming in the subtitle domain

`subtitle_core::Edit::Retime` accepts one `CueTiming` for every existing cue, in
document order. Each entry contains a `CueId` and validated `Milliseconds` for
start/end. It cannot alter cue text, identities or ordering. Missing, duplicated,
unknown or reordered IDs fail before the document is changed. Existing overlaps
are allowed, just as with individual timing edits.

The application calls `Editor::apply_edit` with the captured revision and its
document validator. The editor validates every range, prepares the complete
change, runs the application policy against the proposed document, and then
publishes one revision/history entry. Policy rejection restores the original
document and preserves redo, saved-state identity and cue allocation. An
unchanged proposal creates no history entry. One Undo/Redo restores the whole
retiming while keeping source text intact.

This reuses the editor's existing range-change history; a whole-document retime
retains the before/after cue ranges. It does not introduce a second undo stack or
an asynchronous mutation path. The domain has no filesystem, Tauri, frontend DTO
or media dependencies. Analysis, proposal ownership, final timestamp rounding,
Save As protection and stale generation checks belong to the application layer.

Validate this boundary with:

```sh
cargo test --locked -p subtitle-core --test retiming
```

The tests include complete undo/redo restoration, invalid/stale/duplicate inputs,
policy rejection, no-op preservation and generated sequences of accepted edits.
