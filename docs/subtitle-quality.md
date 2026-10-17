# Subtitle quality

Implementation in progress (2026-10-17). This contract describes Task 011;
the shared checker, CLI and desktop profile Settings are implemented. Live cue
metrics, Problems review and gap acceptance are still in progress. Checking is
read-only and never makes a subtitle document unsavable.

## Metrics and source evidence

The named `vss_utf16_v1` text algorithm independently follows source observations
from VisualSubSync `71778b9115e57c03e9be85234318c2004ac427b3`:
`MiscToolsUnit.pas::TagSplit/StripTags`,
`JavaScriptPluginUnit.pas::GetStrippedText`, and `SRTParserUnit.pas`.
Complete `<...>` and `{\...}` sequences are removed. Other braces, unmatched
openers and entities remain literal. CRLF and LF are normalized; each retained
line break counts as two UTF-16 units for speed, as in the historical host's
CRLF text. Spaces/punctuation count; supplementary characters count twice and
combining marks separately. Per-line lengths exclude newline units. No Unicode
normalization or entity decoding occurs. These are behavioral specifications,
not a claim of exhaustive executed VSS equivalence.

Counting and rendering are separate. SRT video preview renders balanced
bold/italic/underline tags and now passes complete ASS/SSA override blocks to
libass without rounding cue times. See the [tag contract](subtitle-tags.md) for
cue opt-in, literal/malformed handling and fixed SRT preview defaults. The VSS
counting policy can exclude syntax which preview renders literally; it counts
ASS text escapes and drawing instructions outside blocks as source characters,
not rendered layout. Its name/details must remain visible with metrics. SRT
preserves original tags on save. Full ASS/SSA files/styles/metadata remain a
separate scope choice; inline tag support is included in this milestone.

For count N and effective duration D milliseconds, CPS = N*1000/D and
RS = N*1000/(D-500). Empty/whitespace-only text, invalid duration and RS durations
at or below 500 ms have explicit unavailable states, never NaN/Infinity JSON.
Actual duration always uses accepted cue endpoints. `ordinary_v1` uses that
duration for speed. `vss_overlap_v1` uses the chronological preceding cue's
adjusted reading start plus ceil(N*1000/34.95 + 500), clamped to its end and the
current cue interval. Evaluation is iterative, bounding overlap chains without
recursion. Equal starts use stable document order; this ordering is explicit
for otherwise unsorted SRT documents.

The RS allowance/ratings and overlap formula come from
Google Code vss-plugins Hg `48928de4cfa6f28c4b90b51a8c65f30803aab650`,
`jsplugin/common/common.js::{getRsFromLengthDuration,getStartIfOverlapping,
READING_SPEED_DEF}`. Strict rating upper bounds are 5,10,13,15,23,27,31,35.
RS minimum is inclusive and maximum exclusive; CPS/duration/line/gap limits
are inclusive. Display rounding never drives comparisons. Enabled speed checks
report unavailable nonempty metrics as problems. Empty cues have a separate rule.

## Profiles and coverage

Version 2 `.vsfpreset` with kind `quality` is a strict, data-only bounded format.
It declares identity/revision, publisher/source/language, named metric policies,
all rule switches/thresholds, gap and scene timing values, and unsupported
semantics. Unknown versions, fields or algorithms fail before application.
Version 1 remains timing-only and must not replace quality state.

`vsf-default` is an authoring aid, not a broadcaster norm. `sw3-uk-vo-subset`
derives from the pinned archive's `presets/SW3_UK_VO.ini`: strict duration
800–5000 ms, RS [5,35), two lines, single-line preferred 37 and strict 40,
multiline maximum 40, gap 160 ms, cut offsets 120/120 ms. It uses overlap-aware
speed and explicitly lacks full duration exemptions, scene/filter/sticky/sequence,
typography/case/dialogue/tag and linguistic line rules. Its coverage stays partial
even when implemented checks find nothing. No SW1 is synthesized. Built-ins are
immutable identities; customization produces a separate named profile.

Settings → Subtitle quality selects a built-in or imports a version-2 profile.
Its unsupported rules and source/counting policy remain visible. **Customize a
copy** assigns a new identity and enables rule editing. Export writes the complete
draft, including provenance and limitations, without applying it. A blank speed
bound disables that bound; the rule switches are separate.

Profile selection also changes the three shared cue/scene timing thresholds.
Changing those thresholds or importing a version-1 timing preset makes a custom
profile while preserving every quality rule and coverage limitation. Interaction
options such as snapping stay in the timing settings. Apply validates this pair,
atomically saves defaults, and then accepts the current workspace policy. Cancel,
invalid imports and failed writes preserve accepted rules and subtitle history.
A draft opened before the workspace policy changes cannot overwrite the newer
policy; reopen Settings to edit it.

Projects save Rust's accepted effective profile snapshot, including its source,
revision and limitations. Restoring one applies that snapshot only after its media
and document restoration succeeds; it never rewrites global defaults. New media
and project detachment restore defaults. Old settings and projects without quality
rules use the VSF default rules with their existing timing values; reading them
does not rewrite their files. Different legacy thresholds get an explicitly named
custom profile, not an inferred historical broadcaster norm.

Overlaps are half-open intervals: touching is allowed when minimum gap is zero.
Chronological neighbors supply gap metrics. A sweep additionally identifies
nested overlaps against the preceding cue with the furthest endpoint, avoiding
quadratic all-pairs expansion. Each later overlapping cue gets a representative
pair finding. Reports retain stable cue IDs and document-order indices; related
cues are explicit. Profile snapshots and their canonical SHA-256 identify results.

## Planned explicit gap action

After checking is verified, a proposal keeps every cue start fixed and only
shortens an earlier cue's end to next-start minus the effective minimum gap.
No cue is extended, shifted, reordered, deleted or collapsed. Already sufficient
gaps stay unchanged. Nested/ambiguous overlapping neighborhoods are unresolved.
Reject candidates that violate the enabled minimum duration, worsen a speed
violation, or enter a configured scene endpoint exclusion zone. Scene constraints
require current completed data when media is attached; unavailable/partial data
is reported, never assumed clear. Without media, scene constraints are explicitly
not checked and require a visible acknowledgment before accepting a partial plan.

Preview includes old/new endpoints, skipped reasons and changed reading metrics.
Acceptance applies only the displayed safe changes together via one validated
`Edit::Retime`, guards document/profile/context identity, preserves failed-save
and recovery policies, and reruns checking. No action is inferred from profile
loading, opening Problems, resynchronizing, or saving.

## Headless check

Build with `cargo build --locked -p vsf-cli`. The checker requires an explicit
profile; it does not read desktop preferences, launch a GUI or discover FFmpeg.

```sh
vsf check --subtitles episode.srt --profile vsf-default
vsf check --subtitles episode.srt --profile sw3-uk-vo-subset --json
vsf check --subtitles episode.srt --profile ./publisher.vsfpreset --json
```

JSON schema 1 contains the effective profile/hash, complete-policy-coverage flag,
accepted cue metrics, findings and distinct affected-cue count. Cue identities
are decimal strings; indices are zero-based, while human output uses cue numbers
starting at one. Numeric times are exact millisecond projections of the domain's
bounded timestamps. Displayed rounded rates never determine findings.

Exit 0 means no findings with complete requested coverage; 1 means findings;
3 means incomplete coverage, possibly with findings; 2 invalid input/limits;
5 file/output IO failure; 130 cancellation. Unsupported historical rules remain
listed even with zero findings. There is no implicit repair or output SRT.

Pure/CLI checks: `cargo test --locked -p vsf-application -p vsf-cli`.
The 10,000-cue benchmark declares p95 <500 ms over 20 warm release runs,
<64 MiB additional sampled RSS, and <16 MiB serialized report. Run:

```sh
cargo build --locked --release -p vsf-application --example quality-benchmark
python3 scripts/quality-benchmark.py
```

The Python RSS sampler uses macOS/Linux `ps`. It samples the benchmark process
after document construction and includes report serialization, excluding compiler
memory. This does not measure native input, UI interaction or GPU behavior.

Initial development-Mac evidence (2026-10-17, pinned Rust 1.98.1): the 10,000-cue,
1,050,000-byte text fixture passes at p95 5.833 ms over 20 warm release runs,
11,010,048 bytes additional sampled RSS, and 7,824,921 serialized bytes. An
earlier benchmark allocated a full JSON vector on every iteration and failed
the memory gate (162,414,592 bytes additional RSS). The retained comparison
uses the production CLI's streaming serializer and counts the same 7,824,921
bytes without allocating a complete output buffer. Budgets remain unchanged;
this establishes streaming performance, not full-buffer allocation performance.
Desktop paging/interaction evidence is still required. Ignored receipts:
`tests/fixtures/generated/quality/benchmark-v1.json` and
`benchmark-streaming-v2.json` and `benchmark-streaming-v3.json` (final source).
