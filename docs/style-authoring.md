# Style, attachment and conversion milestone

Task022 is in progress. Existing style inspection/assignment and loss-preserving
same-format editing are implemented; the authoring/export capabilities below are
not yet available. This document tracks their implementation and acceptance.

The next increment makes document-level source properties part of Rust's existing
Undo history. A style edit and any required cue-reference changes must succeed or
fail together. The domain stays format-independent; format/application code owns
style validation and source-record preservation. Frontend controls remain drafts
until a reviewed transaction is accepted. Existing cue-only history retains its
range storage, and context-only history must not copy all subtitle cues.

Required subsequent work:

- Create/copy/edit ASS/SSA styles, with precise field validation, safe named style
  references and explicit handling of reset tags and commented events.
- Inspect embedded attachments; add, replace and remove supported TTF/OTF fonts
  with bounded decoding and exact preservation of unrelated/unsupported records.
- Preview deliberate SRT/ASS/SSA conversion, identify lossy or unsupported content
  and timing precision changes, and safely save a separate output file.
- Validate mixed cue/document Undo/Redo, failed transactions/saves, stale proposals,
  preview/recovery/project persistence, font replacement/cleanup, format matrices
  and representative 10,000-cue performance on available environments.

Predeclared gates: pure 10k proposals p95 <=500ms and additional sampled RSS <=96MiB;
native interactions/cancellation/paging/acceptance/invalidation <1000ms. Background
completion is recorded separately under the existing worker deadline. Native
hardware/distribution acceptance remains separately tracked. No new validation
result is claimed by this plan.

## Ownership foundation

`subtitle_core::Editor<P = ()>` now retains immutable source properties in the
same revision/state/history as cue ranges. The desktop uses `Editor<Source>`;
save, preview, recovery and proposal captures read that owner. Cue validation can
borrow the accepted source without cloning it. `apply_document` validates a
complete cue/property candidate before publishing one operation, rejects identity
or order changes, preserves redo on failure/no-op/stale input, and stores only
changed cue ranges. Properties-only edits keep no copies of unchanged cues.

This increment adds no UI mutation path yet. Format-aware authoring and conversion
proposals remain required. ADR0030 remains proposed until the full milestone is
validated. References informing the later controls: [Aegisub styles](https://aegisub.org/docs/latest/styles/)
and [attachment manager](https://aegisub.org/docs/latest/attachment_manager/).

Foundation validation (2026-10-17):

```sh
cargo test --locked -p subtitle-core
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

All pass on the pinned Rust toolchain: 332 ordinary Rust tests, 30 ignored;
zero-test harnesses are not feature evidence. Four new domain tests cover combined
properties/cues, mixed history/saved state, rejected/stale/no-op transactions,
identity/order preservation, unchanged neighbors and empty documents. The owner
case covers both ASS/SSA style+text state, snapshot projection, exact saved bytes,
failed writes, persisted/reopened recovery and whole-state Undo/Redo. Initial
compilation identified two remaining field accesses after moving Source into
Editor; both now use its borrowed accessor. Logs are retained under ignored
`tests/fixtures/generated/authoring-research/`. No native authoring or scale
acceptance is claimed yet; those remain required for Task022.

## Loss-preserving style records

The format adapter now shares immutable base records with a separate style
overlay. Edits replace only the selected style record; additions insert explicit
Format/Style records at the style section boundary (creating that section when
absent). Original field order, padding, line endings and unrelated source remain
intact. Preview headers, default cue metadata and style lookup use the same
accepted overlay. Application field/reference validation is still required before
this lower-level transformation can become a user-facing transaction.

`cargo test --locked -p subtitle-formats` passes 16 tests, including four new
overlay/preservation/history/invalid-record cases.
`cargo clippy --locked -p subtitle-formats -p subtitle-core --all-targets -- -D warnings`
passes after replacing an unnecessary test clone with `slice::from_ref`.
The tests cover both variants, BOM/CRLF, empty/absent/EOF style sections, added
styles with independent columns, exact unrelated records and shared base identity.
Native authoring, field/reference proposals and scale acceptance remain open.
