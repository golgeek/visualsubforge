# WebVTT editing

Stage6a is complete within the recorded Mac scope. The format adapter and application integration are
implemented: desktop open/edit/save, projects, accepted-state recovery, shared
text/timing/quality tools and CLI check/statistics dispatch recognize WebVTT.
Library, owner, browser, native Mac workflow, pure and desktop 10k checks pass. Bounded
native visual observations confirm the preview limitations below, and actual
macOS file choosers pass. The rest of Task023
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
clear the preview. Current-runtime observations below show decoded CJK/voice
text and literal ASS syntax, but omit CSS/region styling, flatten ruby and expose
an emoji missing-glyph notice. Browser-equivalent timed highlighting is unverified.

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

## Acceptance boundaries

- The final desktop 10k save/recovery and interaction measurements below pass.
  Existing <=500ms pure p95, <=96MiB additional sampled RSS and <1000ms interactive
  gates are unchanged; background work retains existing deadlines.
- CSS/region/ruby/emoji and timed-highlight limits remain explicit. Windows/Linux
  feature and physical-device acceptance are separate open release gates.

No new worker, native rendering library or mutable accepted-state owner was added.

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

That application integration run alone does not establish native, VM, physical
device or release acceptance. The subsequent native evidence is recorded below.
Stage6a and ADR0031 remain open, followed by legacy import, manual translation,
plugins and the native/distribution reliability milestone.

## Native Mac workflow and visual observations — 2026-10-18

The native-enabled test build uses the existing private
`audio-integration-master-v1` mpv runtime. It does not adopt a new renderer or
library strategy. The isolated identifier is
`com.golgeek.visualsubforge.webvtt-native-20261018`; its config has the same
test-only capabilities as `tests/desktop/tauri.test.json`. A byte-identical copy
of the generated bundle lives under `test-results/webvtt-native-20261018/` so
independent CUA inspection binds the intended app.

```sh
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype \
  --config "$PWD/test-results/webvtt-native-20261018/tauri.json" -- --locked
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  VSF_WEBVTT_CAPTURE=1 VSF_WEBVTT_CAPTURE_LABELS=webvtt-project-restored \
  node tests/desktop/run.mjs \
  --binary "$PWD/test-results/webvtt-native-20261018/VisualSubForgeWebVTT.app/Contents/MacOS/visualsubforge" \
  --webvtt
```

Final `desktop-nI1oKm` passes the native workflow. Its three-cue BOM/CRLF source
retains identifiers, comments, CSS/region/unknown settings and timed markup.
Formatting remains a draft until Apply; the accepted first-cue start changes
from 1001 to 1002 ms, then reviewed replacement retains its surrounding tags.
Undo/Redo restores complete cue state. Whole-document shifting refuses absolute
markers; a safe selected shift preserves exact 1 ms precision and can be undone.
Cancelled, missing-parent and wrong-format destinations preserve state, and the
successful save/reopen is byte-exact against independently expected output.
Project save/restore retains format, paths and complete cues. A real interrupted
process restores the accepted WebVTT checkpoint, requires Save As, preserves a
newer external file and writes the recovered copy exactly. Desktop caption metrics
match the executable CLI under the same profile; their media/scene coverage is
reported separately. Final normal shutdown is code0.

The executable SHA256 is
`b6bc99e0109ef0396ece37189b9fb505a38bdc4d7cdc3a1b0bb81abfd44e03a6`.
The final runtime receipt identifies PID49487 and 46 loaded backing files,
including that executable and the expected private libmpv, with none unavailable.
Independent CUA imagery after project restoration shows bold `Hiver`, decoded
ampersand/CJK text and the preserved source markup in the editor.

Earlier `desktop-kAAUg4` records three independent settled native-window images
and the same binary/runtime (PID48959, 46 backing files): at 1.5 s, white decoded
voice/language text is bottom-centered instead of the specified lime/region
layout; at 4.5 s, ruby base and annotation share a baseline; at 6 s, ASS alignment
braces and `\N` remain literal while HTML bold and the ampersand decode. CJK
glyphs appear, but U+1F600 is a missing-glyph box with a matching font notice.
These are bounded observations, not browser conformance or timed-highlight
animation acceptance. Original records/text remain unchanged by preview.

That earlier run passed metric parity but failed its next harness step: seeks
correctly followed the active cue, while the harness assumed cue1 remained
selected. The corrected test explicitly scrolls/selects cue1 through the real
list before editing. The failed receipt and images remain retained; the complete
second run supersedes its workflow result without hiding the failure. No product
code changed for this harness correction. Harness lint/format/syntax and the
native-feature build pass. No new Windows/Linux or physical-device acceptance is
claimed; real chooser and desktop10k validation remain separate gates.

### Actual macOS chooser acceptance — 2026-10-18

```sh
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features media-prototype \
  --config "$PWD/test-results/webvtt-picker-20261018/tauri.json" -- --locked
```

This build excludes `desktop-e2e`, global Tauri access and WebDriver. The config
sets only the isolated identifier
`com.golgeek.visualsubforge.webvtt-picker-20261018`. The byte-identical
`VisualSubForgeWebVTTPicker.app` copy has executable SHA256
`90c942e5c2a4d3aafb64acab51fafa470db5ce2cdd95d34b885f03ba13e8df7b`.
The runtime receipt identifies PID51411 and the expected private libmpv among
46 loaded backing files, with none unavailable.

Native CUA input opens the three-cue source through the OS Open sheet, edits
`Été` to `Printemps` while preserving surrounding markup, and applies it. The
File menu correctly disables conversion. Save As shows `source.vtt` in the
source directory; Cancel retains the unsaved edit and retry saves a separate
`picker-copy.vtt`. Independent byte comparison verifies the exact BOM/CRLF,
settings, blocks, timestamps and text replacement; the source hash is unchanged.
Cmd-Q exits normally with code0. Open/Save images, menu/editor accessibility
observations, hashes, runtime and exit receipts are under
`test-results/webvtt-picker-20261018/`.

The initial Go To shortcut/paste sequence reused the chooser's previous path;
observing the path field before setting the exact fixture path corrected it
before any file was opened. Native selection of a substring in the textarea was
unavailable, so ordinary focus/select-all/paste entered the complete known cue.
Neither limitation required a product change. Inspection also found a stale empty
editor hint listing only SRT/ASS/SSA; it now includes WebVTT. The wording change
is checked separately and does not change the identified picker binary above.
Windows/Linux choosers and physical acceptance remain unverified. Desktop10k
validation is the remaining stage6a acceptance gate.

## Desktop 10k validation — 2026-10-18

`scripts/webvtt-fixtures.py` reproduces the three library benchmark inputs and
asserts their recorded SHA256 hashes. The native WebDriver lane runs five
edit/Undo/Redo/save cycles per input, comparing all 10,000 cues after history
operations and exact independently expected bytes after saves. It also checks
full reopen, last-cue navigation with bounded rendered rows, typing during a
background query, cancellation, stale-proposal rejection and real interrupted-
process recovery followed by a separate exact Save As. Original files remain
unchanged. The unchanged limits are <1000 ms for each measured interaction and
ten seconds for preparation; pure CPU/RSS measurements remain separate above.

Retained first run `desktop-0SS3f2` failed a harness selector that expected a
space after the recovery filename; the rendered text contained a newline. The
test now matches the exact source path on the recovery entry. The next run,
`desktop-mjZRM2`, passed plain recovery but measured styled recovery at 1349 ms,
exceeding the declared limit. Its other styled interactions and exact output
checks passed. Both failed receipts and binaries remain available under the
ignored `test-results/` evidence directories; neither run is accepted as a pass.

Investigation found the accepted document was parsed during recovery validation
and then parsed again for adoption. Recovery now transfers that validated parsed
document into the existing editor. Saved and accepted text still both undergo
full format/size validation under the same lease/read operation, with no second
read, cache or new state owner. A regression verifies exact SRT/ASS/SSA/WebVTT
restoration, refusal of either corrupt text field and durable snapshot retention.
The complete workspace passes 399 Rust tests with 30 ignored; strict Clippy,
repository lint and formatting pass. Native remeasurement is required below
before accepting this change against the desktop performance gate.

### Final desktop result

The rebuilt debug app passes `desktop-59LdC7`, including final normal exit code0.
The isolated identifier remains
`com.golgeek.visualsubforge.webvtt-performance-20261018`, with the same test
capabilities as `tests/desktop/tauri.test.json`. Executable SHA256 is
`0d18eb7a4a23d3254bf57b79027621dc466f82dac881e0684799e421fa0e63d6`.
The private runtime selection is unchanged. No media is loaded in the measurement;
generated DOM input, supplied file choices and a final animation frame measure
application response, not physical input or compositor presentation. Desktop
CPU/RSS is not inferred from these timings; the separate pure results above apply.

| 10,000-cue input | Open | Max Apply of five | Max Undo/Redo/restore of five | Max save of five | Reopen | Recovery open | Recovery Save As | Preparation |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Plain Unicode | 257ms | 124ms | 36ms | 143ms | 195ms | 414ms | 98ms | 75ms |
| Styled BOM/CRLF | 536ms | 250ms | 55ms | 254ms | 502ms | 950ms | 257ms | 208ms |
| Timed/mixed endings | 489ms | 222ms | 54ms | 239ms | 471ms | 862ms | 225ms | 137ms |

All other interactions pass: draft/typing <=2ms, cancellation <=16ms, stale
invalidation <=196ms, last-cue navigation <=39ms and recovery-edit acceptance
<=190ms. Each list renders six rows after last-cue navigation. All prepared queries
find 10,000 matches with pages capped at 200 rows. Full cue comparisons, exact
saved/recovered bytes and unchanged source hashes pass for all three workloads.
The styled recovery measurement has only 50ms of headroom: this is bounded
evidence for this development Mac and debug build, not a cross-device guarantee.

With the pinned Node 24.21.0/npm 11.19.0 and Rust 1.98.1 toolchains:

```sh
python3 scripts/webvtt-fixtures.py
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run format:check
npm run lint
node --check tests/desktop/webvtt-performance.mjs
node --check tests/desktop/run.mjs
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" \
  npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype \
  --config "$PWD/test-results/webvtt-performance-20261018/tauri.json" -- --locked
ditto target/debug/bundle/macos/VisualSubForge.app \
  test-results/webvtt-performance-20261018/VisualSubForgeWebVTTRecovery.app
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" \
  node tests/desktop/run.mjs \
  --binary "$PWD/test-results/webvtt-performance-20261018/VisualSubForgeWebVTTRecovery.app/Contents/MacOS/visualsubforge" \
  --webvtt-performance
git diff --check
```

The native build includes strict Svelte/TypeScript checks and the frontend
production build; both pass. Logs are retained in
`test-results/webvtt-performance-20261018/`, with the final native report,
screenshots and exit receipt in `test-results/desktop-59LdC7/`. No UI or IPC
contract changed in this recovery optimization, so the prior 44 frontend and
56 browser cases remain its integration baseline, not newly rerun evidence.
Stage6a and ADR0031 are accepted within this scope. Legacy project import,
translation, plugins and the final native/distribution milestone remain required.
