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

## Shared style proposals

Pure application proposals now create complete ASS/SSA definitions or copies,
edit declared supported fields and rename ordinary named styles. They preserve
unknown columns, existing record padding, cue IDs/order/times and all unrelated
source. A rename updates exact Dialogue assignments, valid Comment assignments
and named reset tags together; repeated renames use the current overlays. Event
style references with leading stars retain those stars. Default's fallback
identity stays stable; create a named copy instead of renaming to/from Default.

Authoring uses explicit limits: 1024 styles, 64 fields/style, 1024 UTF-8 bytes/value,
128 bytes/name and 100,000 cues, within the existing whole-file size ceiling.
Names exclude control/comma/brace/backslash/parenthesis syntax, leading stars and
case variants of Default. Font names fit 256 bytes; font size is >0 through 1000;
scale/outline/shadow 0–1000; spacing ±1000; angle ±360000; margins 0–99999;
charset/SSA alpha 0–255; flag values -1/0/1; border style 1/3. Alignment follows
each variant's numbered set. Colors accept bounded 32-bit hex or signed/unsigned
decimal. Existing unsupported fields are retained read-only. These are authoring
limits, not a claim that every other imported value is invalid ASS/SSA.

Malformed Comment events and incomplete, nested, transformed, padded, spaced,
parenthesized or oversized reset syntax block renaming with an actionable error.
Definition edits that do not rename retain such text and report its reference
coverage limit. Legacy starred definition aliases cannot be edited ambiguously;
a uniquely named copy remains available. Review counts identify known direct cue
references, not simulated renderer output or all possible fallback effects.

Primary source inspection checked [libass 0.17.4 event style lookup](https://raw.githubusercontent.com/libass/libass/0.17.4/libass/ass_utils.c)
and [strict reset lookup/parser](https://raw.githubusercontent.com/libass/libass/0.17.4/libass/ass_parse.c).
Their distinct star/Default/reset handling motivated explicit guards instead of
blind text replacement. No upstream implementation code was copied.

Validation: `cargo test --locked --workspace` passed 340 ordinary tests (30
ignored) before the additional alias regression; strict workspace Clippy passed.
Final targeted `cargo test --locked -p vsf-application --test style_authoring`
passes all 5 tests, `cargo test --locked -p subtitle-formats` passes all 16, and
`cargo clippy --locked -p vsf-application -p subtitle-formats --all-targets -- -D warnings`
passes. Initial compilation exposed unconditional use of the optional Specta
derive; the module now uses the existing `specta` feature gate. No new dependency
was added. UI/owner review integration, native acceptance, attachment management,
conversion and the full scale gate are still required.
