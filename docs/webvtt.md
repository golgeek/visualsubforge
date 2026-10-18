# WebVTT editing

Stage6a is in progress. The pure format adapter is implemented; WebVTT is
not yet exposed by desktop open/save, projects, recovery, CLI or native preview.
The rest of Task023 (legacy projects and translation) remains required.

## Implemented format foundation

The baseline is the [W3C WebVTT syntax and cue model](https://www.w3.org/TR/2026/CRD-webvtt1-20260520/).
Standalone UTF-8 caption files retain their signature/header, optional BOM, cue
identifiers/settings, NOTE/STYLE/REGION blocks, markup, Unicode and original line
endings. Parsed cue text uses LF internally; unmodified source stays byte-exact.
Changed endpoints use exact milliseconds, without centisecond rounding. A cue
edit changes only its selected text/times/metadata; unrelated records remain exact.
Empty header-only files and a final block without a trailing newline are accepted
and preserved. Creating a cue adds only the separators needed to reopen it.

The adapter rejects invalid block boundaries, timestamps, duplicate identifiers,
decreasing cue starts and times beyond the domain's 9999-hour limit. Overlapping
cues remain valid. Segmented HLS `X-TIMESTAMP-MAP` headers are refused with an
explanation because their time mapping has not been implemented. Unknown settings
and markup remain unchanged; this is not a complete conformance validator.

`subtitle_formats::webvtt::parse` produces a domain Document and immutable Source.
The Source shares one original string and indexed ranges through Arc; cloning it
for history does not duplicate every original cue. CueMetadata carries identifiers
and settings. `Editor::apply_edit_with_properties` or `apply_document` runs the
format serializer's validation before committing, so failed validation preserves
revision, saved state and redo. Undo restores exact original bytes. `serialized_len`
uses the same streaming writer as serialization without allocating the output.

Generic metadata now permits commas, which WebVTT settings and identifiers need.
ASS's event serializer retains its delimiter restriction; a regression proves
that a comma-bearing ASS actor field cannot be accepted into history or saved.
No filesystem/native dependency, worker or additional state owner is introduced.

Create/delete and unnamed cue splitting are covered at this boundary. A named
split currently fails duplicate-identifier validation. Application-level split/
merge policies, identifier authoring and their CSS/reference implications still
need explicit handling before desktop exposure.

## Integration contract still to implement

Source preservation does not imply full WebVTT rendering. STYLE/REGION and raw
settings are preserved without executing CSS or claiming a CSS/settings
conformance validator. Preview must state which styling/layout it approximates.
Voice/language/class/ruby markup, entities and internal timestamp tags require
WebVTT-specific text projection; they must not enter the SRT fansub-tag encoder.
Literal ASS braces in WebVTT are ordinary text. Unknown markup must be preserved
and excluded from unsafe automatic fixes.

Cue identifiers/settings belong to the accepted cue metadata and its existing
Undo history. Shared immutable original source records belong to Editor<Source>.
Split/merge must preserve settings and avoid duplicate identifiers; timed payloads
need explicit safe policies. Internal timestamps use absolute media time, so
automatic timing tools must not treat them as cue-relative ASS effects. Manual
edits must validate increasing markers strictly inside the edited interval.

Same-format save must remain protected, and SRT/ASS/SSA conversion must never
silently treat WebVTT as one of those formats.

## Acceptance still required

- Complete application-level structural edit policies and unsupported-content
  reporting, including identifier/style references and timed payloads.
- Format dispatch, bounded IO, save/reopen, projects/recovery and failure/race
  integration through existing owners and typed IPC.
- Safe markup-aware text/quality/timing workflows and explicit preview limits,
  followed by browser and actual native workflow checks.
- Representative 10k parse/serialize/edit/save/recovery and native interaction
  measurements. Retain existing <=500ms pure p95, <=96MiB additional sampled RSS
  and <1000ms interactive gates; background work retains existing deadlines.

No new worker, native rendering library or mutable accepted-state owner is planned.

## Foundation validation — 2026-10-18

On the development Apple Silicon Mac, using Rust 1.98.1 and Python 3.14.7:

```sh
cargo test --locked --workspace
cargo test --locked -p subtitle-formats
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo clippy --locked -p vsf-application --example webvtt-benchmark -- -D warnings
cargo build --locked --release -p vsf-application --example webvtt-benchmark
python3 scripts/webvtt-benchmark.py
git diff --check
```

384 Rust tests pass, 30 native/external integration cases remain ignored. The
format crate passes 30 tests, including ten new WebVTT/metadata regressions:
exact source restoration, BOM and mixed terminators, EOF/empty payloads, Unicode,
time limits, source/property edits, insertion/deletion, safe split rejection,
malformed input, boundary injection, failed-edit redo preservation and 10k cues.
The ASS delimiter regression protects the existing format. Zero-test harnesses
are not counted as feature coverage. Strict Clippy and formatting pass; the
initial Clippy `filter_map_bool_then` finding was corrected and its log retained.

Each release benchmark runs one warmup and 20 measured cycles. A cycle parses
10,000 cues, accepts a last-cue text/timing edit with serialization validation,
serializes and checks the byte count, reparses the complete output, restores the
original bytes through Undo and verifies Redo. Fixture generation precedes the RSS
baseline; parsed state is included. Styled input has BOM/CRLF, identifiers/settings,
CSS/region/comment records and markup; timed input adds absolute timestamp markers
and mixed terminators. Fixture hashes are emitted in the receipt.

| 10,000-cue workload | Input bytes | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Plain Unicode | 682,922 | 22.330ms | 24.456ms |
| Styled Unicode | 2,052,100 | 51.506ms | 51.852ms |
| Timed/mixed endings | 1,838,692 | 48.166ms | 48.518ms |

Additional sampled RSS is 26,230,784 bytes across 180 observations; all cases pass
the predeclared 500ms/96MiB limits. Sampling is not exhaustive allocation
accounting. The first benchmark fixture accidentally combined CR and LF across an
empty line, removing a required separator; the parser correctly refused it. The
generator was corrected, with no parser relaxation. Failure and final receipts
are retained under `test-results/webvtt-foundation-20261018/` (ignored local evidence).

This validates the pure format/history boundary only. No new frontend, native
renderer, save/recovery, VM or physical-device acceptance is claimed. Existing
desktop formats remain SRT/ASS/SSA until the integration acceptance above passes.
