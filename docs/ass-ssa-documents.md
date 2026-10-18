# ASS/SSA document integration

Task 016 is **in progress**. Format/history, desktop persistence, style controls
and format-aware text/timing tools are implemented. The complete user workflow is
not yet accepted: actual Mac desktop workflows, font-availability checks and
10,000-cue desktop interaction measurements remain required.

## Preservation policy

The pure `subtitle-formats` adapter accepts UTF-8 ASS v4.00+ and SSA v4.00 with an
optional BOM. No-op serialization preserves all bytes, including mixed LF/CRLF,
final-newline choices, comments, unknown fields/sections and attachment source
records. Edited dialogue changes only its changed fields. Physical editor line
breaks become literal `\N` when accepted into an ASS/SSA cue. SRT retains its
existing physical-newline and cue-local escape policy.

Declared columns can be reordered and case-insensitive; Text must be last. Start,
End, Style and Text are required. Unsupported versions, invalid UTF-8/NUL, bare
CR, malformed dialogue and ambiguous layouts fail before replacing the document.
ASS/SSA timestamps use a 10 ms grid; unrepresentable edits are rejected rather
than rounded at save. SRT retains 1 ms precision.

Rust cues own accepted event metadata. Existing range-based history therefore
restores styles, layer, margins, actor, effect and unknown fields together with
text/timing. A combined style/text/timing edit is one transaction; changing text
with an unchanged style name preserves the original Style field whitespace.
The adapter shares immutable original records through an Arc, without a mutable
metadata sidecar. No frontend, filesystem or native dependency enters the domain
or format adapter.

New cues use Default, or the first existing style, with zero layer/margins,
Marked=0 for SSA and otherwise empty metadata. Splits copy metadata. Merges require
matching metadata and insert an ASS line break/style reset. Structural operations
reject cue-relative effects, ambiguous markup and unsafe merged positioning or
mixed SRT/ASS contexts. The current resynchronization operation explicitly accepts
only SRT; extending its alignment/publication policy is separate from these repairs.

## Style editing and safe tools

The cue editor's **Cue style** selector changes a draft. **Apply edit** commits
style, text and timing together, so one Undo restores all three. **Inspect styles…**
shows original style fields and script settings without assigning or editing them;
opening it preserves the current draft. Undefined existing style names remain
preserved but must be replaced with an existing definition before using formatting
shortcuts. There is no style designer or font attachment manager.

Typed endpoints must be representable exactly. Playhead, creation, splitting and
waveform gestures quantize to the source grid before committing; waveform previews
retain at least one format tick. File pickers accept SRT/ASS/SSA on Open and offer
only the current format on Save As. Linux portal changes have not run on Linux.

Full ASS/SSA text uses escape syntax even without an override block. Brace comments
and tags are protected; HTML-looking text is literal. Find/Replace maps visible
text to its original source and refuses replacements crossing protected tokens.
Replacement cannot introduce breaks; use the editor or line tools. Join/Reflow can
change hard/soft break whitespace, emits `\N`, and preserves other tags, escapes
and event metadata. Its wrapping uses visible UTF-16 lengths; the historical
quality-counter policy remains unchanged and does not model rendered typography.
Timed/drawing/unknown markup, positioned layout and nonempty event effects are
manual-review skips. Existing tagged-SRT behavior remains covered separately.

Bold/Italic/Underline shortcuts insert ASS override spans and restore the assigned
style's base value afterward. Rust resolves that value using a captured document
generation, revision, cue and draft style. Existing controls of that property or
style resets require manual editing, except toggling an exact generated span.
Formatting changes only the frontend draft; stale results cannot replace it.

Gap repair floors the latest permissible end to the format grid. Duration/speed
repair intersects exact bounds with that grid and selects the closest valid end,
preferring the earlier endpoint on a tie; an empty intersection is an explicit
precision skip. Scene repair moves endpoints inward to representable safe edges.
Exclusion windows are expanded to adjacent grid points and strictly overlapping
expansions merged once, retaining every representable safe point. All three
operations preserve metadata, skip timing-sensitive tags/nonempty event effects
and retain whole-document safety validation and atomic Undo/Redo. Source format
is captured alongside the document under the existing lock; evaluation stays on
the existing cancellable workers.

`vsf check` and `vsf stats` use the same format-aware read boundary. They preserve
the named metric policy and never rewrite the input. Missing scene coverage remains
explicit, as for SRT.

## Save, projects and recovery

Source format is retained by Save and Save As. A conflicting known extension
fails before touching files; ASS/SSA requires its matching extension. Cross-format
conversion remains deferred. Existing atomic-save, external-change and recovery
Save As protections apply.

Projects gain an optional subtitleFormat field, validated against their path and
referenced content. Old projects infer identity from the referenced file. Rust
captures identity under the existing document-to-project publication lock; an
unsaved, pathless document does not create a project subtitle reference.

Recovery version 4 carries explicit ASS/SSA identity and exact saved/accepted
source text. The legacy saved_srt/accepted_srt JSON names remain for compatibility;
their content follows the declared format. Existing SRT versions 1–3 remain
readable, and ordinary new SRT records retain version 3. Validation precedes
atomic checkpoint replacement. Recovery preserves the source and requires Save As.

## Native preview and current evidence

The style/tool increment passes 299 ordinary workspace Rust tests, 41 frontend
unit tests, 48 Chromium/WebKit cases, strict Clippy/lint/format/type/IPC checks and
the default desktop build. A focused browser rerun also passes after list-key
lint corrections. These include format-aware public APIs, desktop owners and CLI
parity; browser interactions use modeled IPC. Receipts are under
`tests/fixtures/generated/ass-ssa/tools-v1/`. Actual Mac workflow acceptance remains
required.

Full documents use their own script information/styles and normalized event
packets through the existing bounded Matroska memory transport. The
[Matroska subtitle mapping](https://www.matroska.org/technical/subtitles.html#ssaass-subtitles)
defines packet order independently of source Format order; SSA has an empty layer.
Unknown source fields remain saved even when the renderer does not use them.
Attachment management and reliable embedded-font availability remain unverified.
The 4 MiB preview transport cap is unchanged and never prevents editing/saving.

2026-10-17 backend tests cover both formats, selective edits, style assignment,
Undo/Redo, save/reopen, extension/grid/style errors, external-change protection,
project identity, recovery, metadata-preserving structure and timed-effect refusal.
FFprobe independently validates ASS/SSA headers/events and tagged SRT's exact
millisecond packets. Software mpv/libass composition shows source-defined red
top titles, white bottom text, line breaks and expiry for both document variants.
The tagged-SRT regression also passes mixed HTML/SSA, motion, color, expiry and
30 track replacements. These are not actual desktop/GPU/audio acceptance.

The software composition fixture's generic sans-serif font cannot resolve the
two CJK glyphs on this Mac runtime: the image shows fallback boxes and the log
records font lookup failures. Unicode source/packet preservation passes; this
run does not establish Unicode font rendering. Retain this limitation for the
native desktop/font-availability checks rather than substituting document fonts.

Reproduce independent native checks after the ordinary Rust tests:

```sh
cargo test --locked -p visualsubforge native_demux -- --ignored --nocapture
python3 scripts/check-ass-documents.py --mpv /absolute/path/to/mpv --output ./tests/fixtures/generated/ass-ssa/composition-new
python3 scripts/check-ass-preview.py --mpv /absolute/path/to/mpv --output ./tests/fixtures/generated/ass-ssa/tagged-srt-new
```

Evidence is retained under `tests/fixtures/generated/ass-ssa/`. Fixtures in
`tests/fixtures/subtitles/` are synthetic, authored in this repository.

## Predeclared format performance

`scripts/ass-fixtures.py` produces 10,000-cue ASS (1,266,177 bytes) and SSA
(1,245,925 bytes) inputs with two styles, Unicode, tags/escapes, event metadata and
opaque records. The exact hashes and recipe were recorded before measurement.
Only raw input bytes are loaded before the RSS baseline. The release example
times application parsing, one accepted style/text/timing edit with source-format
validation and serialization, then verifies exact Undo outside the CPU timer.

One warmup and 20 measured runs per variant pass the unchanged p95 <500 ms budget:
ASS 27.813 ms; SSA 24.048 ms. Additional sampled RSS is 48,250,880 bytes, below
96 MiB, across 109 observations. `budget-v1.json` retains hashes and results.
This measures format CPU/RSS; desktop interaction and native presentation remain
separate gates.

```sh
python3 scripts/ass-fixtures.py
cargo build --locked --release -p vsf-application --example ass-benchmark
python3 scripts/ass-benchmark.py
```
