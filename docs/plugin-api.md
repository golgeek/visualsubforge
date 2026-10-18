# Initial plugin API and VSS compatibility

This is the implemented **application-library subset**, with a tested
[local CLI workflow](plugin-cli.md). The desktop owner/typed transport and
[review controls](plugin-desktop.md) now exist; complete desktop acceptance
remains pending. It is not a claim
of complete VisualSubSync compatibility. The [host guide](plugin-host.md) records
runtime limits and remaining integration gates; ADR0008 remains proposed.

## Captured packages

`vsf-application::plugins::Package` contains a manifest and a sorted map of relative
JavaScript filenames to captured UTF-8 source strings. The library does no IO.
The separate `plugin_files` application module performs bounded local package IO
and protected publication for the CLI. The examples under `examples/plugins/` use
`manifest.json` plus `main.js`. No installed-plugin directory or package installer
exists yet; the CLI and desktop require an explicitly selected local manifest.

Manifest version 1 requires `id`, `name`, `api` (`vsf-1` or `vss-1`) and `entry`.
`requires` and scalar `settings` are optional. IDs use lowercase ASCII letters,
digits, dots and hyphens, at most 64 bytes; display names are nonblank, at most 256
UTF-8 bytes. Packages contain 1–64 `.js` files and at most 1 MiB of source in total.
Paths are at most 255 bytes, relative and canonical, with no empty/dot/parent
components, backslash, colon, NUL or wildcard. Case-folded filename collisions
are rejected. Source content, filenames, API, requirements and settings all enter
the deterministic SHA256 package fingerprint.

Requirements are exact, unique names: `rules`, `fixes`, `core-read`, `scene-data`,
`package-scripts`, `stripped-text-write`. Unknown requirements fail before running.
`scene-data` also requires available completed, mapped scene metadata. These are
host capabilities, not inferred compatibility with a dated historical VSS pack.

Settings are at most 64 named booleans, finite numbers within JavaScript's safe
numeric range, or strings of at most 4096 UTF-8 bytes. Names are at most 128 ASCII
letters/digits/underscores; prototype-reserved names are rejected. Modern manifest
settings declare defaults. VSS defaults come from `VSSPlugin.Param*.Value`, then
manifest and invocation settings override them in that order. Overrides must name
an existing setting and retain its scalar type. No automatic type coercion occurs.
The report includes the initial effective settings. Plugin-local state can change
during a run but is discarded afterward.

`LoadScript(path)` evaluates an explicitly captured script in shared globals.
During loading, paths resolve relative to the current file, including `..` within
the package; callback-time loads resolve relative to the entry directory.
Backslashes in load requests normalize to `/`. Missing files, root escape,
absolute/wildcard paths, cycles and nesting beyond 16 fail the whole invocation.
Repeated noncyclic loads evaluate again. There is no fallback search, downloaded
dependency, filesystem or native module access.

## Invocation and modern API

A capture contains the accepted document/source, package, generation/revision/
context key, cue scope, selection, optional cursor, effective preferences and
optional completed scene data. Up to 100,000 cues are supported, subject also to
the 16 MiB subtitle and 32 MiB wire-message bounds. Cue IDs and revision keys travel as
decimal strings so JavaScript number rounding cannot alias identities.

Register `globalThis.VSFPlugin = { check(cue, host), fix(cue, host) }`.
`check` is required and returns one string finding, empty string or null. `fix` is
optional. Check mode never calls it; fix mode calls it only after a nonempty
finding. Calls follow document order within the requested scope. Initialization
and checks cannot edit. Findings are limited to one per scoped cue; fixes may
update other captured cues, and the review marks those outside the original scope.

| Modern `host` / global `VSF` member | Contract |
| --- | --- |
| `version`, `sourceFormat` | `1`; `srt`, `ass`, `ssa` or `vtt` |
| `preferences` | Immutable minimumBlankMs, minimumDurationMs, maximumDurationMs, cpsTarget and nullable rsTargetIndex |
| `cursorMs`, `settings` | Nullable integer cursor; fresh immutable effective-setting snapshot |
| `getCue(id)` | Immutable `{id,index,startMs,endMs,text}` snapshot; index is 1-based; unknown ID fails |
| `getNext(id)`, `getPrevious(id)` | Snapshot in file order, or null at an end |
| `getAllCues()`, `getSelection()` | Immutable arrays of snapshots in file order |
| `getSceneCuts()` | Immutable completed mapped millisecond cuts; unavailable data fails |
| `update(id, patch)` | Fix-only partial replacement of `text`, `startMs`, `endMs`; other fields fail |
| `log(message)` | Same bounded diagnostic sink as `ScriptLog` |

Cue text uses LF. Snapshot objects do not update themselves after a write; call
`getCue` again to read current transaction state. Times must be nonnegative exact
integer milliseconds, no greater than 35,999,999,999. Temporary intermediate
endpoint combinations are allowed inside a fix; Rust validates the final pair.
Promises, queued asynchronous jobs and promise-returning fixes are unsupported.

## VSS subset matrix

Register the traditional global `VSSPlugin` object with `Name`, `HasError` and
optional `FixError`. Both callbacks receive `(CurrentSub, PreviousSub, NextSub)`
and `this === VSSPlugin`. Missing neighbors are null. Wrappers retain identity
within an invocation and always read the latest transaction values, including
earlier callbacks' writes. Nothing survives the next helper invocation.
`Name` supplies the report name. Legacy `Description`, `Color`, `Message` and
parameter `Unit`/`Description` metadata do not currently affect execution or
presentation. General-plugin hooks are not called by this rule/fix host.

| VSS surface | Implemented behavior |
| --- | --- |
| `Start`, `Stop` | Writable during fixes; exact signed 32-bit nonnegative integer milliseconds |
| `Text` | Raw text, exposing LF as CRLF; writes normalize CRLF to LF |
| `Index` | Read-only, 1-based file-order index |
| `StrippedText` | Legacy tag-removal getter; setter is the raw `Text` replacement alias |
| `VSSCore.GetSubCount`, `GetSubAt` | Count and 0-based access; out-of-range access returns null |
| `GetFirst`, `GetNext`, `GetPrevious` | Stable captured wrappers, null at boundaries |
| `GetSelectedCount`, `GetFirstSelected`, `GetNextSelected` | Captured selection traversed in file order |
| `GetAudioCursorPosition` | Captured cursor; unavailable cursor fails |
| `MinimumBlank`, `MinimumDuration`, `MaximumDuration`, `CpsTarget`, `RsTargetIndex` | Explicit captured preferences; unavailable RS target fails |
| `SceneChange.GetCount`, `GetAt` | Completed cut count, 0-based access; invalid range returns -1 |
| `GetNext`, `GetPrevious`, `Contains` | Inclusive next/previous and inclusive range containment; no cut returns -1 |
| `StartOffset`, `StopOffset`, `FilterOffset` | Captured mapped scene offsets, including zero |
| `ScriptLog`, `LoadScript`, `Param*.Value` | Bounded diagnostics, captured dependency loading, scalar settings |
| Actions, columns, UI/selection mutation, split hooks, GDI text measurement, filesystem/native access | Unsupported; unknown `VSSCore`/`SceneChange` members fail explicitly |

Unavailable scene data differs from an available empty cut list. Only the latter
returns a zero count and no-cut sentinels. VSS captures also reject cursor,
preference and scene times outside signed 32-bit range. There is no silent clamp
or guessed Delphi fractional-number coercion; use the modern API for longer times.

The stripped getter independently recreates the pinned Pascal scanner's observed
source contract: remove closed `<...>` and recognized `{\...}` blocks, retain
entities/ASS escapes/CRLF, and stop stripping at the first unclosed opener.
The Pascal minimum-length rule for a short ASS block is retained. This is not
VSF's readable-text projection. Assigning stripped text can remove formatting;
the proposal exposes the raw before/after text and a markup-loss warning.

Checks retain ordinary JavaScript UTF-16 string counting and global RegExp state
across callbacks. Unsupported API use or an invalid setter poisons the invocation
even when a script catches its exception. Diagnostic messages/findings cap at 1024
UTF-16 units; logs cap at 100 entries and a conservative 16 KiB estimate. Rust also
validates returned byte limits and all result fields independently.

Legacy file-decoding failures are not part of this compatibility contract. VSF
retains valid UTF-8 text, including supplementary Unicode characters. The
[historical probe](../tests/fixtures/plugins/vss-contract/README.md) uses an
equivalent UTF-16 input where needed to bypass a legacy VSS loading failure;
that workaround does not change VSF's subtitle or plugin string behavior.

## Acceptance boundary and ownership

The application produces `Execution { script, input }`. The caller runs that in
the [bounded helper](plugin-host.md), then consumes the capture to validate its
reply. Only stable cue IDs, raw text and endpoints may change; styles, attachments,
event metadata and order stay in the original source. Rust rejects malformed or
duplicate edits, invalid durations/text, source precision loss, oversized output
and reversal of an originally chronological document. ASS/SSA endpoints must be
exact multiples of 10 ms; SRT/WebVTT use 1 ms. CRLF normalizes to LF and ASS/SSA logical
newlines become `\N` at the existing format boundary.

Timed/ambiguous markup is checked before and after a proposed change. Unsafe
duration changes fail; WebVTT timed payloads also reject shifts. Removing tags in
the same proposal cannot bypass the original-text guard. Raw markup changes are
visible review warnings; no hidden safe-text interpretation is substituted.

`Proposal` owns its immutable capture and validated rows. Inspection returns
borrowed views, with no mutable route to replace accepted edits. `edit()` requires
the current document, source and complete request/context to match. The owner
must still hold its document lock and pass the captured expected revision to
`Editor`; an edit followed by Undo does not make an old proposal current again.
`Edit::UpdateBatch` validates the complete ordered text/timing batch and preserves
metadata/properties in one history entry. Failed domain/format/size validation
preserves document, history, saved identity and redo.

Captures and history share immutable cue metadata fields through `Arc`; a style
replacement copies its fields before changing them. This reduces repeated event
field allocations without introducing a mutable metadata owner. VSS wrapper
lookup uses the captured array index, while accessor functions are shared across
stable wrappers; their own enumerable properties and write guards are retained.

No engine/native dependency enters the domain or application crate. The CLI now
composes the shared application with the worker through an injected executor;
plugin-host tests/examples also compose those boundaries. The desktop now owns
one plugin worker/no queue and private validated proposals, with tested admission,
paging, revision/context guards, single Undo and existing save/recovery integration.
Package verification reserves acceptance off-lock; media→scene→document checks
protect final commit. Frontend review/draft reconciliation, browser races and
actual Mac format/history/save/recovery/chooser workflows now pass their recorded
checks; ten complete desktop10k workloads now add full review/history/persistence
and race coverage. The [desktop guide](plugin-desktop.md) records the subsequent
Windows/Linux ARM64 workflows; the historical comparison below closes the bounded
stage7 acceptance. Physical-platform and broader compatibility gates remain separate.

## Provenance and validation scope

The two trailing-whitespace examples are independently authored and covered by
the repository GPL-3.0-or-later license. They do not contain legacy implementation.
The optional `unchanged_archived_overlap_rule_and_fix_run_locally` test reads an
external file selected by `VSF_LEGACY_OVERLAP_SCRIPT`, requires exact SHA256
`56a698e5569fdce0d9103d25708a992aa299a68f5879fc8c5186be655b85b278`, and does not
copy or redistribute it. That archived rule's checker and three integer-safe fix
modes pass locally, including neighbor review and one Undo; an odd overlap that
produces fractional endpoints is explicitly refused. This does not establish
parity with a running historical VSS engine by itself. The independently pinned
host comparison below and Windows/Linux ARM64 desktop workflows now complete
stage7 acceptance within the documented checker/fix subset. See the
[platform scope](plugin-desktop.md).

The independently authored [cross-host contract probe](../tests/fixtures/plugins/vss-contract/README.md)
uses ES3 syntax, three synthetic Unicode/tagged cues and explicit context. Its VSF
CLI control passes read-only checking and combined text/timing/neighbor-fix review,
including raw/stripped UTF-16 reports and callback/RegExp state. The historical
comparison below records the observed cases separately. No historical
executable, DLL or unresolved-license plugin is included with the probe.

## Historical VSS comparison — 2026-10-18

The first actual historical checker comparison on 2026-10-18 uses the separately
pinned executable identified in the probe README. Its exported report has SHA256
`9bb45c27c1bdc7cc485b8ccadc65d27c2044f060976a87a18054bf5722d419e1`.
After matching its observed preferences (120/1000/6000 ms, CPS18, RS index0),
empty selection, cursor0 and empty scene list with offsets130/130/250 ms, 63 of
66 field values match across three cues. These include raw/stripped UTF-16 text,
endpoints, indexing/neighbors/bounds, selection traversal, cursor/preferences,
scene sentinels/offsets, callback/RegExp progression, parameter and receiver.
The sole differing field is `GetSelectedCount()` with no selected cues: the
historical executable reports `null`; VSF deliberately retains numeric `0`.
The legacy HTML exporter also writes the emoji as CESU-8 surrogate sequences,
despite declaring UTF-8. Comparison extracts the probe's ASCII hex fields from
the original bytes, without silently replacing invalid text. Neither encoding
failure becomes a VSF compatibility requirement. Receipts and exact check/preview
commands are in `test-results/plugin-legacy-host-20261018/` under
`historical-check-comparison.json`.

The user then applied **Fix all errors of same type (color)**. Save As produced
no visible dialog; its cause remains unverified. After verified backups, ordinary
Save produced the historical file (SHA256
`57bcdb7243ada1b03fc7f45cb0e7cb73a7d53631276986d66d433a1cdc0060d1`).
All three cues exactly match VSF's reviewed and actually published result:
1010–1910, 3010–3910 and 5010–5910 ms; `!` on the first cue and ` neighbour!`
on the other two, preserving the original tags, Unicode and line break. This
also observes writes to the next cue in later callbacks. The original UTF-8
fixture remains byte-identical.

The historical SRT repeats the CESU-8 exporter defect. Comparison explicitly
decodes paired surrogate sequences and normalizes CRLF to LF; it performs no
character replacement or timing tolerance. VSF's published file is valid UTF-8,
SHA256 `d5cd5cd83e21f02965210fddc27ed5a2404e5019ac45d56381d3ca875de31795`,
and its entire decoded serialization matches after that newline normalization.
Receipts are `historical-save-receipt.json`, `historical-fix-comparison.json` and
`historical-apply-comparison.json`; `compare-fixed.py` records the exact semantic
comparison. CLI check returns1 for three findings, preview/apply return0, and the
comparison exits0. The CLI SHA256 is
`4381e697c92c6dd6c168fbc24ce6798f9c27e6e9e1f3170b23c2520318f66260`.

Together with the recorded worker/API/history, native/VM and complete10k checks,
this completes the initial checker/fix milestone and accepts ADR0008 for that
scope. It does not establish every historical plugin pack, nonempty scene/selection
case in this three-cue probe, or equivalence between the supplied binary and the
pinned source checkout. Those limits and the unsupported API matrix stay explicit.
