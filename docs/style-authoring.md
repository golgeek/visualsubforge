# Style, attachment and conversion milestone

Task022 is in progress. Reviewed ASS/SSA style creation, copying and editing now
join style inspection/assignment and loss-preserving same-format editing. Reviewed
font attachment management is also validated on the recorded Mac runtime.
The [pure conversion foundation](format-conversion.md) is implemented; desktop
review and protected separate-file publication remain required. This document
tracks implementation and acceptance; the full milestone is not yet complete.

Document-level source properties now participate in Rust's existing Undo history. A style edit and any required cue-reference changes must succeed or
fail together. The domain stays format-independent; format/application code owns
style validation and source-record preservation. Frontend controls remain drafts
until a reviewed transaction is accepted. Existing cue-only history retains its
range storage, and context-only history must not copy all subtitle cues.

Milestone scope (style and attachment increments accepted below; conversion pending):

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


### Attachment record foundation

The pure format adapter now exposes bounded metadata inspection (up to 200 rows,
with full total count), streams original encoded lines without payload copies, and
supports separate shared font replacement/addition overlays. Original graphic and
ambiguous records remain read-only. Font edits share base source and authored
payloads across history and unrelated style edits. Save size accounting and preview
headers consume the same overlays; removing a block preserves surrounding records
and newline state. Canonical encoded lines that happen to start/end with brackets
remain font data rather than being mistaken for section headers.

Validation: `cargo test --locked -p subtitle-formats` passes 20 tests, including
four new both-variant attachment inventory/stream/overlay/history/boundary/limit
cases. `cargo test --locked -p vsf-application --test style_authoring --test subtitle_documents`
passes all 6 tests; `cargo clippy --locked -p subtitle-formats --all-targets -- -D warnings`
and formatting pass. Test payloads at this layer exercise record mechanics, not
valid decoded fonts. That record-layer validation does not cover font decoding or rendering;
the application validation increment is recorded below. Desktop controls and
native font replacement/cleanup remain required.


### Font payload foundation

Application `authoring::fonts` now performs bounded streaming ASS decoding,
canonical encoding and conservative single-font sfnt inspection. It checks the
four-MiB bound, table directory/count/order/alignment/ranges/checksums, whole-font
checksum, required tables, TrueType glyph-location bounds, CFF/CFF2 header shape,
horizontal metrics and bounded family-name records. Collections, invalid encoding,
truncated/overlapping/out-of-range tables and unsupported trailing data are
refused. These are structural checks, not a complete font sanitizer or proof of
rendering. Original malformed/unsupported source remains preservable.

The original TTF fixture decodes byte-for-byte. Two new independently authored
CFF/CFF2 fixtures share its family and swap its simple shapes; no third-party font
outlines were copied. `scripts/font-fixture.py` regenerates all using pinned
fontTools4.61.1 in an ignored isolated generation environment. No fontTools or
native font dependency was added to the application. Existing TTF/ASS fixture
bytes remain unchanged. CFF SHA256 is
`b6d34a45311d3a001236a74794b46733a7f85dc9e20b0a80f72806fbe7adb1ae`;
CFF2 is `48d612ceb1106fdd2d46a0fd2a4faaf61ae2fdef0bb3424ab0f93732c75a60b9`.

`cargo test --locked -p vsf-application --test font_attachments` passes three
cases covering all three structural font variants, exact codec round trips,
all remainder lengths, padding, size/cancellation and malformed sfnt data.
`cargo clippy --locked -p vsf-application --all-targets -- -D warnings` passes.
The first test compile used the facade serializer on the lower-level ASS source;
it now uses that module's serializer. Initial Clippy findings use the pinned
Rust toolchain's array-chunk API. Font inspection/proposals still need desktop
integration and native replacement/cleanup acceptance; this foundation does not
complete attachment management.


### Reviewed attachment proposals

The shared application now supplies a bounded inventory and immutable add,
replace and remove proposals. Inventories contain metadata, decoded byte counts,
SHA256 and structural font information, never payloads. Supported font changes
retain cue values and unrelated source records and commit through the existing
`Editor<Source>` history. Exact same-name/same-byte replacement is a no-op and
preserves redo. Cancellation remains an operation error even when it occurs
inside a parser that otherwise reports unsupported input as a read-only row.

The limits are 200 displayed records (partial inventories cannot authorize edits),
32 active fonts, 4 MiB per decoded font, 8 MiB total decoded fonts, and the existing
16 MiB complete subtitle limit. Adding/replacing requires every existing font
record to be supported; this avoids claiming a reliable total for ambiguous
blocks. A supported font can still be removed while preserving unsupported peers.
Graphics stay read-only. Names are bounded `.ttf`/`.otf` filenames with no path or
control characters. Candidates exceeding the existing 4 MiB preview limit report
that limitation before acceptance; save eligibility remains separate.

`cargo test --locked -p vsf-application --test attachment_authoring --test font_attachments`
passes all seven tests. Coverage includes ASS/SSA, BOM/CRLF, opaque graphics,
TTF/CFF/CFF2, exact unrelated bytes, history/no-op redo/reopen, partial/malformed
inventories, every one-shot cancellation checkpoint, count/individual/aggregate
limits and complete-file refusal. A synthetic valid sfnt with an additional opaque
table exercises the exact 8 MiB aggregate boundary independently of invalid-font
rejection. Strict application Clippy and formatting pass. The first new proposal
test accidentally duplicated the fixture's Events section; the fixture now uses a
valid recognized font-section delimiter. No production parser rule was relaxed.
This increment still has no desktop attachment controls or native acceptance.

Full `cargo test --locked --workspace` passes 355 ordinary tests with 30 explicit
manual/generator harnesses ignored. Zero-test doc harnesses are not feature
validation. Logs are retained under ignored `authoring-research` fixtures.


### Desktop font review implementation

The cue editor's **Manage fonts…** dialog now inspects the current ASS/SSA
attachments and previews add, replace or remove operations. The review shows
filenames, family, decoded sizes and SHA256 before/after, complete-file size and
any preview-size limit. Unsupported records are read-only, and a partial inventory
cannot authorize a change. Cue drafts must be resolved before preparing changes.

`font_start/view/cancel/accept` are thin typed command adapters. Inspection and
file reads reuse the existing text/style document-review worker, with one job,
no pending queue, global identities, cancellation, a cooperative ten-second
deadline, stale generation/revision/policy checks and joining outside the document
lock. Inspection captures only the shared source; change proposals also capture
cue state after admission. Font files must be regular, bounded files; reads check
cancellation between 64 KiB chunks. Filesystem calls themselves are synchronous
on that worker, so an unresponsive filesystem can delay a checkpoint. No blocking
read runs on the UI thread.

The candidate owns the reviewed bytes. Acceptance never rereads the selected
file: it validates and commits the document/source pair through existing history.
Frontend selection, inventory metadata and review identity are transient;
`DocumentSession.acceptFont` is the sole frontend adoption path. Picker and
acceptance lifetimes are protected, and late replies cannot revive a closed or
changed document. The shared Font job holds its larger edit proposal behind a
Box; this keeps ordinary inventory jobs small without changing source ownership.
Mac/Windows and Linux portal chooser paths both offer TTF/OTF filters. This Mac
build does not establish Windows/Linux chooser acceptance.

Twelve focused owner tests pass (`cargo test --locked -p visualsubforge document::texts`),
including three new font cases with both-format add/replace/remove, exact save,
no-op redo, failed-save protection, recovery, changed files after review, cross-
family guards, stale policies/documents, bounded reads, cancellation and shutdown.
Eight frontend font workflow/lifecycle assertions pass in both Chromium and
WebKit. The initial strict Clippy run identified the proposal enum's size; boxing
its edit variant resolves that finding. A modeled frontend client's parameter was
narrower than the typed boundary; correcting the test signature restores strict
checking. Native attachment replacement/cleanup and 10k acceptance still remain.


### Font management acceptance — 2026-10-18

The attachment increment is complete within the tested Mac runtime scope;
Task022/ADR0030 remain in progress/proposed for conversion. This does not close
Windows/Linux installation or physical GPU/audio gates.

Full workspace validation passes **358 ordinary Rust tests** (30 explicit manual
or generator harnesses ignored), **44 frontend unit tests**, and **54 Chromium/
WebKit browser cases**. Strict workspace/native-feature Clippy, Rust formatting,
ESLint, Svelte checking, frontend build/format and generated IPC checks pass.
The native app builds with the existing private OpenGL/audio-integration runtime.
Its tested bundle executable SHA256 is
`0e480ab0f01a156021b335682e942c885d36dd8b2c880e30c0b772f2ef16af37`.

Actual desktop `desktop-Km4h1C` passes ASS/SSA add/replace/remove, picker cancel,
invalid-font refusal, one-step history, exact Save As/reopen, failed-save
preservation, project restoration, property-only durable crash recovery and
external-file conflict/separate-copy protection. Each variant reuses one native
player for attachment replacement/removal. Actual native-window CUA observations
(the exact bundle path, not a separately running older prototype) show the TTF's
triangle/block, replacement CFF's block/triangle, ordinary AB after removal,
restored CFF after Undo and after project reopening. The frame's generation and
revision are checked before each observation and retained in the acknowledgment.
Webview review PNGs separately show the readable metadata and proposed-change
review; they do not capture the sibling video. The final application quits with
code 0. Intentional recovery SIGKILLs retain the previously documented tiny
fallback-link resource risk; font payloads themselves use the existing in-memory
preview, with no font extraction/installation. This is observable replacement/
removal and normal lifecycle evidence, not an exhaustive renderer-allocation audit.

Software composition `fonts/composition-authoring-v1` passes nine controls,
including authoring-produced CFF and CFF2 packets with platform fonts disabled.
Both select their embedded PostScript names and display the independently authored
shapes; neither falls back. Attachment-free control is black. The CFF2 image was
visually inspected. The probe reports libass 0.17.5 in this loaded runtime, and
checks private fallback resources are absent after each normal probe exit.
This software lane adds CFF2 format coverage; it does not claim physical GPU proof.

The eight pure inventory/add/replace/remove × ASS/SSA 10k workloads pass the
unchanged 500ms/96MiB gates: maximum p95 **11.569ms**, additional sampled RSS
**72,433,664 bytes**, 180 samples. Each operation has 20 measured runs after one
warmup, uses original small TTF/CFF fixtures (1220/1296 bytes), and times proposal,
acceptance and serialization; Undo/Redo checks are outside that timer. Inspection
has its own measured workload. Full-size font/aggregate/file limits are separately
validated by the ordinary large-font tests, not represented as 10k native timing.

Corrected native scale receipt `desktop-ir5a6l` passes all six add/replace/remove ×
ASS/SSA cases, five accept/Undo cycles each. Maxima: open 317ms, inventory 429ms,
background preparation 376ms, acceptance 129ms, Undo 101ms, cancellation 1ms and stale
invalidation 107ms. Zero-millisecond samples mean clock resolution, not zero cost.
All interactions remain below 1000ms; preparation remains below the separate
10-second worker limit. Corpus and attached-input hashes are retained in receipts.
Final normal quit is code 0. The first scale receipt `desktop-2hrkgj` met timing
limits but failed final quit because the test's direct IPC edit/Undo left the
frontend snapshot stale. The guard test now edits and undoes through the actual
UI; no production acceptance rule or budget was relaxed. Both receipts remain.

Reproduce after the pinned native build from the Mac guide:

```sh
cargo build --locked --release -p vsf-application --example font-benchmark
python3 scripts/font-benchmark.py
cargo test --locked -p visualsubforge native_font_preview_sources -- --ignored --nocapture
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" python3 scripts/check-preview-fonts.py --probe /tmp/vsf-font-preview-probe --output tests/fixtures/generated/fonts/composition-authoring-new
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" node tests/desktop/run.mjs --fonts --font-performance --binary target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
```

The probe compilation command remains in preview-fonts.md. For native visual
observations, set `VSF_FONT_DESKTOP_CAPTURE=1` and optionally comma-separated
`VSF_FONT_DESKTOP_CAPTURE_LABELS`; read `capture-request.json` and acknowledge its
matching label only after observing the exact app's native window. This lane has
a 60-second observation limit. Routine native runs omit the pause. Logs and JSON
scale reports are copied to ignored `authoring-research` fixtures.
