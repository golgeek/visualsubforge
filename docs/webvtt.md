# WebVTT editing

Stage6a is in progress. The format adapter and application integration are
implemented: desktop open/edit/save, projects, accepted-state recovery, shared
text/timing/quality tools and CLI check/statistics dispatch recognize WebVTT.
Library, owner, browser and pure 10k checks pass. Actual native workflow, renderer,
file chooser and desktop 10k acceptance remain pending. The rest of Task023
(legacy projects and translation) remains required.

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

Create/delete and unnamed cue splitting use the existing history owner. Named
cues cannot be split or merged automatically: identifiers must remain unique and
may be referenced by CSS. Compatible unnamed cues can merge when their settings
agree and joined markup retains its scope; independent unclosed voice spans
require manual closing first. Identifiers, settings, STYLE and REGION records
are preserved read-only; this increment does not provide their authoring UI.

## Text, timing and preview policies

Source preservation does not imply full WebVTT rendering. STYLE/REGION and raw
settings are preserved without executing CSS or claiming a CSS/settings
conformance validator. The editor exposes a preview limitation notice. Native
transport sends a bounded WebVTT presentation copy directly to the existing
decoder, normalizing only its BOM/line endings; it never enters the SRT fansub-tag
encoder. The 4 MiB preview limit leaves editing/saving available. Empty documents
clear the preview. Actual current-runtime visual acceptance remains pending.

Shared WebVTT text projection understands b/i/u, classes, voice/language labels,
ruby and internal timestamps. Quality counts decoded caption text in UTF-16,
with each physical newline counted as CRLF for speed and excluded from line
length. Voice/language/class labels, ruby annotations and timestamp markers do
not count; ruby base text does. Literal ASS braces and backslashes count as text.
Reports identify this as `webvtt_utf16_v1` in optional `textInterpretation` fields;
the UI and human CLI output expose that interpretation. Existing SRT/ASS/SSA
reports retain their historical algorithm and JSON shape.

Terminated HTML named and numeric references decode for display-text counting and
search, including two-codepoint names. Their original source spans are protected
against automatic replacement or partial formatting. Unknown/malformed syntax or
unterminated references remain literal and require manual repair. Balanced source
mapping supports Unicode-safe Find/Replace, b/i/u draft formatting, spelling and
reviewed corrections. Voice closing can be omitted for a span covering the whole
cue, and the final ruby annotation end tag can be omitted before `</ruby>`.
Settings and ruby/timed layout prevent automatic line reflow. Text transformations
retain existing 64 KiB cue limits; streaming metrics avoid a full character map.
Nesting above 64 levels and recognized tags over 4096 bytes require manual review.

Cue identifiers/settings belong to the accepted cue metadata and its existing
Undo history. Shared immutable original source records belong to Editor<Source>.
Internal timestamps use absolute media time. Batch shifts, gap/duration/scene
repairs and nonidentity resynchronization refuse or skip timed/ambiguous cues
with a review explanation. Untimed cues retain millisecond precision; changes
that reorder starts are rejected. Manual timing edits validate increasing markers
strictly inside the edited interval. Split/merge also refuses timed payloads.

Same-format saves retain atomic publication, external-change protection and
failed-write/history preservation. Recovery retains source format and blocks,
requires Save As and does not restore Undo history. Project references retain
their existing identity checks. Conversion from WebVTT is explicitly unavailable
in both native menu and editor; the application boundary also rejects it.

## Ownership and dependency walkthrough

Open detects or constrains the format through `subtitle-formats::Source`; the
existing bounded application IO and desktop document owner adopt `Editor<Source>`.
Frontend format metadata selects ordinary text/time edits with 1 ms precision,
without showing ASS style controls. Rust validates the complete candidate before
accepting it into existing history. Formatting remains a frontend draft until
Apply, using generation/revision/cue context captured under the document lock.
Save, recovery, project references and preview all read the accepted source/cues.

`vsf-application::subtitle_markup::webvtt` supplies one bounded interpretation for
source-mapped tools and streaming metrics. Named entities use the pinned
`web_atoms = 0.2.6` table from [Servo/html5ever](https://github.com/servo/html5ever),
licensed MIT OR Apache-2.0. This was already locked transitively; the application
now declares it directly without upgrading dependencies. Only its static tables
are used, not an HTML execution environment. Domain and format crates acquire no
native or IO dependency. Existing proposal/checker workers, cancellation,
generation/revision guards and off-lock evaluation remain unchanged; no new
worker, mutable document sidecar or native library is introduced.

## Acceptance still required

- Actual Mac workflow through the native-enabled app: open/edit/format, reviewed
  text/timing, exact save/reopen/history/project/recovery and failure/stale paths.
- Current-runtime visual observations for supported text, literal ASS syntax,
  timestamps, ruby, CSS/regions and explicit preview limits; actual file choosers.
- Desktop 10k save/recovery and interaction measurements. Retain existing <=500ms
  pure p95, <=96MiB additional sampled RSS and <1000ms interactive gates;
  background work retains existing deadlines.

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

This historical foundation run validates the pure format/history boundary only.
The subsequent integration evidence below does not establish native acceptance.

## Application integration validation — 2026-10-18

Pinned Rust 1.98.1, Node 24.21.0/npm 11.19.0 and Python 3.14.7 on the development
Apple Silicon Mac. All commands run from the repository root, with `.nvmrc`
selected before npm commands:

```sh
cargo test --locked -p vsf-application --test webvtt --test styled_proposal
cargo test --locked -p visualsubforge webvtt
cargo test --locked -p vsf-cli --test webvtt
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run ipc:generate
npm run ipc:check
npm run format:check
npm run lint
npm test
npm run build
npm run test:browser
npm run test:browser -- --grep 'workflow: ass-documents'
npm run tauri -- build --debug --no-bundle -- --locked
cargo test --locked -p vsf-cli
cargo clippy --locked -p vsf-cli --all-targets -- -D warnings
cargo build --locked --release -p vsf-application --example webvtt-benchmark
python3 scripts/webvtt-benchmark.py
git diff --check
```

The final workspace run passes 398 tests, with 30 external/opt-in cases ignored.
Coverage includes bounded input and format detection; exact source restoration;
Unicode/entity/ruby/voice interpretation; protected search, formatting and spelling
corrections; stale proposals; timed shift/resync/gap/duration/scene safety; unnamed
structural edits and explicit named-cue refusal; same-format save/reopen, wrong
extension/failed IO/external conflicts; project identity and recovery preservation;
native transport bounds and cancellation during large-cue metrics. Executable
`vsf check` and `vsf stats` match shared WebVTT reports and preserve input bytes,
without FFmpeg/FFprobe access. A final CLI rerun covers updated help descriptions.

44 frontend tests and all 56 Chromium/WebKit browser cases pass. The production
App workflow includes four WebVTT checks alongside existing ASS/SSA regression:
format-specific controls/notice, exact ordinary edits, Undo/Redo and formatting
that remains a draft until Apply. The final targeted rerun passes both engines
after adding native-menu state and delayed-action protection. These workflows
use modeled IPC and do not establish actual native menu, filesystem or rendering
acceptance. Static checks, generated IPC consistency and the default desktop
build pass; a shell build does not validate native playback.

The expanded release benchmark retains one warmup and 20 measured cycles for
each exact 10,000-cue fixture. Each cycle includes the foundation parse/edit/
serialize/reopen/history work plus whole-document reviewed replacement, complete
before/after quality evaluation and one atomic text-batch acceptance. Two Undo
operations restore exact original bytes; two Redo operations restore the complete
accepted output. No filesystem or UI latency is included.

| 10,000-cue workload | Input bytes | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Plain Unicode | 682,922 | 48.144ms | 48.494ms |
| Styled Unicode | 2,052,100 | 100.174ms | 101.171ms |
| Timed/mixed endings | 1,838,692 | 92.620ms | 92.748ms |

Additional sampled RSS is 54,722,560 bytes over 327 observations, within the
unchanged 500ms/96MiB limits. Fixture hashes and individual receipt scope remain
in `test-results/webvtt-integration-20261018/benchmark.json`. Sampling is not
exhaustive allocation accounting or desktop responsiveness measurement.

A separate decoder probe used `/opt/homebrew/bin/ffmpeg` 9.0.2:

```sh
/opt/homebrew/bin/ffmpeg -version
/opt/homebrew/bin/ffmpeg -v error -i tests/fixtures/subtitles/styles.vtt \
  -f ass test-results/webvtt-integration-20261018/decoder-output.ass
```

It decoded entities/voice labels, converted internal markers to karaoke timing,
flattened ruby annotations inline and omitted the fixture's CSS/region styling.
This is evidence for that standalone decoder only; it does not identify the
private approved mpv runtime or prove visual presentation. The editor's notice
states the potential CSS/region/ruby/timed-text differences without promising
browser fidelity. Current-runtime native observations remain required.

Retained failures/corrections: the first integration run changed legacy ASS error
wording and failed two existing assertions; format-specific wording restored the
contract. The new browser harness initially clicked Undo before reconciliation
enabled it; it now waits for authoritative UI readiness. The new executable test
initially omitted mandatory `--profile`; its invocation was corrected. After
updating timing explanations, IPC drift correctly found stale generated samples;
regeneration restored consistency. These receipts and final logs are retained
under `test-results/webvtt-integration-20261018/` (ignored local evidence).

No new native, VM, physical device or release acceptance is claimed. Stage6a and
ADR0031 remain open, followed by legacy import, manual translation, plugins and
the native/distribution reliability milestone.
