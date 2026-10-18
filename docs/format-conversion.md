# Deliberate subtitle conversion

Stage5 is in progress. Shared proposals, the desktop review/export dialog and
protected new-file publication are implemented. Native workflow and desktop 10k
acceptance remain required. Ordinary Save keeps its same-format restriction.

Open **File → Convert Subtitles…** or use **Convert subtitles…** in the cue editor.
Apply or discard an unapplied cue draft, choose a different output format, then
select **Review conversion**. Exact timing is the default. Read the loss and
blocking findings before choosing a new output filename; blocked reviews cannot
export. Changing options requires another review. Export completion shows the new
path and leaves the original document open, with its saved state and Undo/Redo
history unchanged. Open the exported file separately to edit it.

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

The desktop owner reuses the existing text/style/font worker slot for both
preparation and subsequent publication. Each has a cooperative ten-second
deadline; no pending queue or sixth CPU worker is added. Typed start/view/cancel/
publish commands expose only bounded reports, status and the published path.
The dialog requires review before destination selection.

Publication shares immutable reviewed bytes through Arc, reserves the same job
slot and prepares/fsyncs a sibling temporary outside the document lock. It checks
cancellation between 64 KiB writes. It uses the existing cross-process writer
lease and refuses existing paths, including directories, hardlinks and dangling
symlinks. Deleted provenance paths remain protected through canonical parents and
conservative case/Unicode-normalization comparison. Namespace/source protection
is rechecked after temporary IO. Under the document lock, a final job/generation/
revision/policy/path check precedes atomic no-clobber publication. This operation
does not adopt a document or mark it saved; source contents changing on disk do not
replace the reviewed output bytes. Failure retains the review for another path.

Cancellation before commit removes the owned temporary. After successful commit,
late cancellation leaves the completed file and publication receipt intact.
Shutdown cancels and joins outside the document lock. The IO worker captures a
Weak store reference, avoiding an owner/worker cycle. It upgrades only for final
publication; if the last external owner disappears then, Job disposal must avoid
joining the current exiting thread. Ordinary blocking filesystem calls cannot be
interrupted mid-call; checkpoints run between them. Crashes may leave an orphan
temporary, and atomic publication does not promise directory power-loss durability.

`ConversionSession` owns options, the report projection and request epochs only.
Snapshot identity, unapplied drafts and option changes disable export immediately;
Rust additionally checks its captured policy epoch and source path. The native
picker rechecks the captured frontend state before dispatch. The dialog remains
open while choosing a path or awaiting publication. Dispatch acknowledgment is
not completion: it polls the authoritative result, including after a lost reply,
and preserves successful receipts when publication wins a late cancellation race.
It never adopts a document snapshot. Closing cancels preparation or retires the
finished review; explicit draft Apply still uses `DocumentSession`.

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

No conversion dialog or native appearance acceptance is claimed by this foundation.

Desktop owner/publication increment, 2026-10-18:

```sh
cargo test --locked -p visualsubforge document::texts::conversion -- --nocapture
cargo test --locked -p vsf-application --lib
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
source "$HOME/.nvm/nvm.sh" && nvm use --silent
npm run ipc:generate
npm run check
npm run lint
npm run format:check
npm run build
npm run ipc:check
```

All final checks pass: **374 ordinary Rust tests, 30 ignored**. Six conversion
owner tests cover all six format pairs, exact reviewed bytes, dirty state/Redo/
paths and persisted recovery, blocked/wrong-family/admission guards, failed-path
retry, competing destination writes, writer-lease refusal/retry, changed source
bytes, deleted provenance, cancel/document/policy/Save-As/close/shutdown races and
last-owner drop. New shared tests cover cancellation at every write checkpoint
and absent-path aliases; existing resync publication tests retain their error
classification and no-clobber behavior through the shared path helper. Actual
Serde samples cover reviewable and blocked reports in generated TypeScript.

Initial tests used nonexistent capture/setter helpers; they now exercise the real
recovery image/persistence and quality operations. An invalid test profile ID
triggered abrupt owner disposal and exposed a strong store/worker cycle ending in
self-join. Weak ownership, current-thread disposal protection and the explicit
last-owner-drop regression fix that lifecycle defect. The corrected owner tests
and complete workspace pass. UI/browser/native conversion acceptance and desktop
10k interaction measurements remain pending; existing frontend unit/browser
counts still belong to the prior authoring increment.

Desktop dialog/browser increment, 2026-10-18:

```sh
source "$HOME/.nvm/nvm.sh" && nvm use --silent
npm run ipc:generate
npm run check
npm run lint
npm test
npm run build
npm run test:browser -- --grep 'workflow: (conversion|ass-documents)'
npm run test:browser
npm run format:check
npm run ipc:check
cargo test --locked -p subtitle-formats
cargo test --locked -p visualsubforge desktop::workspace_event_preserves_name_and_native_menu_payloads
cargo test --locked -p visualsubforge document::texts::conversion -- --nocapture
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" cargo clippy --locked -p visualsubforge --features desktop-e2e,media-prototype --all-targets -- -D warnings
cargo fmt --all -- --check
npm run tauri -- build --debug --no-bundle -- --locked
```

The full suites pass **374 ordinary Rust tests / 30 ignored**, **44 frontend unit
tests** and **56 browser cases** across Chromium and WebKit. The new conversion
page executes 13 workflow/lifetime checks per browser using actual Serde report
samples and modeled IPC. It covers draft Apply/failure/discard, exact refusal,
explicit nearest review, fansub notices, report escaping/bounds, picker cancellation,
failure/retry, all destination filters/path suggestions, original state/history,
preparation cancellation, stale options/path/drafts, single-flight polling, late
admission and picker disposal, lost dispatch/status replies and late publication
receipts. The production App fixture additionally covers toolbar/menu dispatch,
modal video occlusion and focus restoration. Browser events/IPC do not validate
native dialogs, actual filesystem publication or GPU presentation.

The initial browser run corrected a whitespace-sensitive report assertion and a
fixture which had not declared its native surface available/above the WebView.
It then exposed real shared-authoring focus restoration before the return button
was re-enabled; close now clears the modal gate and waits for that DOM update
before focusing. Type/lint corrections supplied an explicit optional-argument
default in the fixture and a stable format-option key. A full parallel Rust run
exposed an existing cleanup-test race: Stale becomes visible before the cancelled
worker has finished removing its temporary. The helper now waits for reaping
before filesystem assertions, while the controlled test separately proves stale/
cancelling status is visible with the worker still held. Production cancellation
and publication policy is unchanged. Corrected focused and full suites pass.

Strict checks, generated IPC drift, default desktop build and native-feature
Clippy pass. These compile on the development Mac; the changed Linux portal
branch has not been compiled or executed here. Conversion native workflow,
appearance, actual destination picker and desktop10k measurements remain pending.
Logs, including the initial browser/focus/lint and Rust cleanup failure receipts,
are retained under ignored `tests/fixtures/generated/authoring-research/`.
