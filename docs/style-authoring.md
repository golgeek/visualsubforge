# Style, attachment and conversion milestone

Task022 is in progress. Reviewed ASS/SSA style creation, copying and editing now
join style inspection/assignment and loss-preserving same-format editing. Font
attachment management and deliberate conversion remain required. This document
tracks implementation and acceptance; the full milestone is not yet complete.

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


## Desktop style review

Open **Inspect styles…**, then **Edit style**, **Copy style** or **New style**.
Resolve an unapplied cue draft first. Existing unknown columns remain read-only;
copy retains them, while new styles use complete variant-specific defaults.
**Preview style changes** reports field before/after values, exact cue/Comment
reference changes, known references and ambiguous-reset coverage. **Accept style
changes** commits the entire candidate as one Undo step. Invalid values explain
the refusal; changing a form retires its old review. Script settings remain
read-only. Ordinary Save retains the original format.

The existing text worker now has explicit Text/Style request and result variants,
shared monotonic IDs, one worker and no queue. Wrong-family reads/accepts fail;
wrong-family/old cancellation cannot cancel a new job. Admission checks the current
document and busy worker before capturing immutable inputs. Computation runs off
lock with cancellation checkpoints and the existing ten-second deadline. Capture,
publication and acceptance use generation/revision/policy epoch. Shutdown cancels
and joins outside the document lock. No additional CPU worker was introduced.

Style acceptance calls `Editor<Source>::apply_document`, revalidating the complete
candidate before publishing source properties and cue changes together. Save,
preview and recovery therefore read the same accepted pair. A no-op preserves
redo and revision. The frontend StyleSession holds only form drafts, captured
identity and bounded review projections; DocumentSession serializes adoption and
reconciles the selected cue/style. Pending acceptance protects dialog lifetime;
late form/start/read replies cannot revive a disposed or obsolete review.

Automated integration validation (2026-10-17):

```sh
cargo test --locked -p visualsubforge document::texts
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm run build
npm run lint
npm test
npm run ipc:check
npm run format:check
npm run test:browser
npm run tauri -- build --debug --no-bundle -- --locked
```

344 ordinary Rust tests (30 ignored), 44 frontend unit tests and all 52 Chromium/
WebKit cases pass. Three new owner cases cover both variants' form/defaults,
reviewed rename/copy, exact save/reopen, failed saves, mixed history, no-op redo,
wrong-family guards, stale document/policy/replacement, cancellation/admission and
shutdown. Eight new browser checks cover draft protection, preserved unknown
fields, exact changed-field requests, failed/successful acceptance, copy/new,
changed forms, late replies and disposed starts. Real Rust Serde form/review
samples compile against generated IPC. Zero-test harnesses are not feature tests.
Initial test compilation corrected the serializer access and policy-change test;
strict TypeScript corrected the field binding and an overly narrow test fixture.
Clippy caught a test-only re-export below a test module; it was moved before it.

The actual native style workflow and desktop 10k checks pass; details follow.
Two initial runs correctly showed the invalid-font-size error but their test
selector read the hidden confirmation dialog. The first attempted selector fix
missed a formatter-inserted line break; the corrected selector targets the open
dialog. A third run needed whitespace-tolerant matching of the correct reference
counts. Receipts desktop-SzlBH6, desktop-4mLr5k and desktop-jEVHUk remain retained.
No production relaxation or budget change was made for these harness failures.

### Style proposal scale

```sh
cargo build --release --locked -p vsf-application --example style-benchmark
python3 scripts/style-benchmark.py
```

The six create/edit/rename × ASS/SSA workloads use the exact existing 10,000-cue
fixtures (ASS SHA256 `00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c`,
SSA `0d2415e1e2d61776e7c9051c3c3c1c10b1c02770e225652b7b0132c57fb79b14`).
The corpus assigns Title to 2,500 cues. Each workload performs 20 measured runs
after warmup; timings include proposal, atomic acceptance and serialization, with
exact Undo/Redo verification outside the timer. Original bytes precede the RSS
baseline; parsed inputs, history and proposals count toward sampled memory.

All pass unchanged 500ms/96MiB gates: maximum p95 25.332ms (ASS rename), sampled
additional RSS 77,709,312 bytes (213 samples). The first run's test expectation
incorrectly assumed 5,000 Title assignments; inspection of the fixture generator
confirmed 2,500. The corrected assertion preserves the exact input hashes and
budgets. The failed run is retained. This is library evidence, not native input
or presentation latency. Attachments/conversion require their own later checks.


### Actual Mac style acceptance

```sh
source "$HOME/.nvm/nvm.sh" && nvm use --silent
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype --config "$PWD/tests/fixtures/generated/fonts/tauri.native.json" -- --locked
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" node tests/desktop/run.mjs --styles --style-performance --binary target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" cargo clippy --locked -p visualsubforge --features desktop-e2e,media-prototype --all-targets -- -D warnings
```

All pass. Receipt `test-results/desktop-Tfv0es` identifies binary SHA256
`d96c66ccd1f85db27f794d9db9c2f676e97cbeaa12c415e245e318e17275e4ac` and the
unchanged private OpenGL/audio-integration-master-v1 runtime. Both ASS/SSA cases
exercise canonical creation, copying, invalid-field rejection, reviewed rename
with Dialogue/reset/Comment changes, combined Undo/Redo, failed Save As,
byte-exact save/reopen, project restore and a property-only edit persisted through
abrupt owned-process termination. Recovery retains the accepted style, protects
external changes and saves a separate exact copy. Native preview generation/
revision acknowledgment is checked; webview screenshots were inspected for form/
review layout. This is not compositor/physical-input or Windows/Linux acceptance.
Normal application quit returned 0.

Native 10k uses the same hashed corpus as the library benchmark: five reviewed
2,500-reference renames per format with full Undo. Maximum ASS/SSA open times are
319/282ms; typing <=11ms, acceptance <=133/134ms and Undo <=100ms. Cancellation is
0/1ms at the webview clock's resolution (zero means below that resolution), and
stale invalidation <=58/53ms. Cancellation/stale rejection use actual owner IPC;
form/preview/acceptance/Undo use generated DOM events. All remain below the
unchanged one-second interaction gate. Complete proposal preparation is recorded
separately at <=343/358ms under the existing ten-second deadline.

This accepts the style-authoring increment on the available Mac. Stage5 remains
open for attachment inspection/management, deliberate conversion and their full
format/history/recovery/native/scale validation. ADR0030 remains proposed until
that complete scope passes. Logs are copied under ignored
`tests/fixtures/generated/authoring-research/`.

## Next: bounded attachment management (not implemented yet)

The next increment retains original attachment blocks and shares authored payloads
across source/history snapshots. Inspection will identify font and graphic records,
with explicit unsupported/malformed coverage. Add/replace/remove will initially
support single sfnt TTF/OTF fonts, bounded to 32 fonts, 4 MiB per decoded font and
8 MiB aggregate decoded fonts, still within the existing 16 MiB document ceiling.
Font collections and opaque/malformed/graphic attachments remain preserved and
read-only. Font bytes will be read on the existing review worker, with no system
font installation, filesystem extraction or decoded font data sent to the UI.
The existing 4 MiB native subtitle-preview ceiling remains separately visible.

Pure format code will own record boundaries/encoding, while application code
checks sfnt table-directory structure and prepares a reviewable source-property
transaction. Structural checks do not certify every font table or renderer.
Payloads share immutable storage so style edits do not copy attachment bytes into
every history entry. Replacing/removing fonts must be validated against actual
preview replacement and cleanup before this increment is accepted.

Primary references: [Aegisub's attachment limitations](https://aegisub.org/docs/latest/attachment_manager/),
[libass 0.17.4 decoding and section handling](https://raw.githubusercontent.com/libass/libass/0.17.4/libass/ass.c),
and [OpenType sfnt directory specification](https://learn.microsoft.com/en-us/typography/opentype/spec/otff).
Traditional SSA specifies TTF attachments; OTF support will be documented as a
modern renderer extension, not a legacy SSA compatibility guarantee. In libass,
only recognized section headers end font parsing; an unknown section following
font data can be treated as payload. The inspector must not silently treat that
ambiguous boundary as a safely editable font. This is design scope, not evidence
of implemented attachment functionality.
