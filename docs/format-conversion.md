# Deliberate subtitle conversion

Stage5 is in progress. The shared, pure conversion proposal is implemented;
desktop review, destination selection, protected publication and native acceptance
are still required. Ordinary Save keeps its existing same-format restriction.

## Output and loss policy

Conversion prepares a separate UTF-8 file with LF endings, no BOM and canonical
records. It leaves the accepted document/source pair and Undo history unchanged.
The immutable proposal owns the exact prospective bytes and a report. A blocked
report has no publishable bytes. Reports group exact occurrence counts by reason
and show at most eight examples per reason, each clipped to 240 Unicode characters;
the examples are explicitly incomplete. No paths, payloads or full documents are
sent as review metadata.

| From → to | Retained/transformed | Reported differences |
| --- | --- | --- |
| SRT → ASS/SSA | Cue order, representable endpoints, Unicode and fansub override bytes. Balanced b/i/u becomes ASS overrides; line breaks become `\N`; literal braces/backslashes are protected with the same encoder as native preview. | Explicit Default style (sans-serif, 24 pt), 384×288 canvas and script defaults. Unsupported/unbalanced HTML remains literal. Literal backslashes can acquire invisible U+2060 word joiners. SSA extensions require a compatible player. |
| ASS/SSA → SRT | Cue order and exact endpoints. Override/drawing/karaoke bytes and ASS escapes are retained as fansub SRT. Escape-only cues receive neutral `\r` opt-in. | All style definitions, script settings, event metadata and attachments are lost. Named resets, positions and style-dependent effects may render differently. Braced comments are removed; contained overrides remain. Literal `<` gets U+2060 protection against accidental HTML interpretation. Players without ASS-in-SRT support can show raw syntax. |
| ASS ↔ SSA | Style names/common fields, mapped alignment/colors, event style/actor/margins/effect, script settings and unmodified encoded attachment payloads. | Missing style fields receive explicit defaults. Fields outside the target schema, Marked/Layer differences, comments, non-Dialogue events and private/unknown sections are reported as losses. Unsupported overrides remain unchanged with a compatibility notice. |

This is a canonical export, not a promise of identical renderer output. The
original source remains the preservation copy. SRT exports do not flatten drawing
commands into dialogue or pretend to reproduce a lost stylesheet. New styles and
fonts accepted through authoring overlays are included in the captured source.

ASS alignment uses numpad positions; SSA uses its historical numbering. SSA →
ASS maps AlphaLevel onto primary/secondary/outline, uses BackColour RGB for both
outline and shadow and gives shadow alpha 128. ASS → SSA uses primary alpha and
outline RGB for the corresponding shared SSA properties, reporting independent
color/alpha losses. Unsupported color/alpha/alignment values block conversion.
This mapping follows the inspected [libass 0.17.4 style parser](https://raw.githubusercontent.com/libass/libass/0.17.4/libass/ass.c);
it is a defined compatibility policy, not a claim about every historical player.
See also [Aegisub override documentation](https://aegisub.org/docs/latest/ass_tags/).

Exact timing is the default policy. Non-centisecond endpoints block ASS/SSA
output until the user explicitly chooses nearest-10-ms rounding or edits them.
Nearest rounds ties upward and reports affected cues with before/after times;
durations, gaps and overlaps may change. Collapsed/out-of-range cues block output.
A changed duration with cue-relative effects or ambiguous markup also blocks it;
equal endpoint shifts preserve duration and leave tags untouched. This uses the
same conservative markup analysis as other timing operations.

## Bounds and ownership

The existing 16 MiB input/output ceiling applies, with at most 100,000 cues and
64 KiB input text per cue. Cross-script style translation supports 1024 styles,
64 fields/style and 1024 UTF-8 bytes/field. Cross-script attachment retention
requires a complete inventory of at most 200 attachments with known boundaries.
It copies encoded lines without decoding, installation or internal font validation.
Ambiguous boundaries block retention; SRT may deliberately discard the entire
inventory with an exact count and bounded names in its report.

Pure preparation borrows Document/Source, checks cancellation between bounded
units and retains only the finished output/report. `subtitle_text` is shared by
conversion and the existing native packet writer; the packet container remains
behind the desktop media boundary. No native dependency, new accepted-state owner,
undo stack or worker is introduced by this library increment.

Desktop integration will reuse the existing bounded text/style/font worker, retain
generation/revision/policy checks and require review before choosing a separate
destination. Publication must use exact reviewed bytes, refuse existing files and
source aliases (including a deleted original path), and check staleness again at
the publication boundary. File failure/cancellation must retain the original
document, dirty state, history and recovery identity. These checks are pending.

Predeclared gates remain 10k pure preparation p95 ≤500 ms and additional sampled
RSS ≤96 MiB; native interaction/cancellation/review/publication <1000 ms, with
background preparation separately bounded by the existing worker deadline.

## Validation

Pure foundation, 2026-10-18:

```sh
cargo test --locked -p vsf-application --test conversion
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release --locked -p vsf-application --example conversion-benchmark
python3 scripts/conversion-benchmark.py
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" cargo clippy --locked -p visualsubforge --features desktop-e2e,media-prototype --all-targets -- -D warnings
```

All pass: eight new conversion cases, 366 ordinary Rust tests and 30 ignored
integrations/helpers. Zero-test harnesses are not feature validation. The cases
cover all six format pairs, exact/nearest precision, rounding collapse/range and
timed-effect refusal, Unicode/HTML/drawings/karaoke/escaped braces, all nine
alignment positions, SSA colors, explicit losses, original/source-overlay/history
preservation, byte-identical decoded embedded fonts, graphics, ambiguous/partial
attachment inventories, one-shot cancellation at every checkpoint, bounded report
samples, empty documents and expanded output above 16 MiB.
Strict native-feature compilation also passes; this does not execute native media.

The scale benchmark retains the existing complete 10k ASS/SSA corpora and adds an
independent 10k SRT mix (balanced HTML, fansub karaoke, literal Windows paths and
plain Unicode). Every SRT endpoint is 1 ms past a centisecond; nearest shifts both
ends equally, while exact refuses. Six output and two refusal workloads each have
20 measured runs after warmup. Timing includes validation, loss reporting and
output serialization; output reopen/count/time checks occur outside the timer.
Source bytes precede the RSS baseline; parsing, preparation and reopened outputs
count toward sampled memory. All unchanged gates pass: maximum p95 **37.489 ms**,
additional sampled RSS **81,526,784 bytes** (415 samples). SRT SHA256:
`ac467ca8e93a1f14bb97e9666e41aa602499a0d06f531d367affba8c5df1ff77`.

The first passing measurement used a temporary Document to construct the SRT
input. The final generator writes identical canonical bytes directly before the
baseline, avoiding retained construction allocations; all input hashes match and
both receipts are retained. Initial compilation corrected a metadata vector type,
the font decoder test call and the shared preview helper's visibility. No behavior
or performance budget was relaxed. Logs/receipts are retained under ignored
`tests/fixtures/generated/authoring-research/`.

No desktop conversion, protected publication or native appearance acceptance is
claimed by this foundation. Existing frontend/browser acceptance remains at the
previous authoring increment; those tests have not been rerun for new UI here.
