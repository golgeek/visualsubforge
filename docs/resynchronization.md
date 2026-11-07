# Subtitle resynchronization implementation

The first resync implementation and its defined acceptance checks are complete.
A headless CLI calls the shared operation. The desktop backend owns proposal
review pages, acceptance and persistence protection, with a Svelte review workflow
under Tools. The [corpus guide](../tests/fixtures/resync/README.md) records the
matcher’s measured range and limitations. Local ARM64 checks cover three OSes;
the native resync walkthrough covers the development Mac. Hosted/x64 CI, wider
content evaluation and native distribution/GPU/audio acceptance remain separate.

Timing policy: every cue start and end follows the measured content map. This
scales short gaps and long pauses when programme speed changes, without deriving
speed from the nominal FPS ratio. Timing profiles and their minimum gaps/scene
buffers are separate constraints; resync does not silently enforce a preset.
Audio matching remains a possible follow-up, not part of this release.

## Source formats and timed effects

UTF-8 SRT, ASS v4.00+ and SSA v4.00 use the same GUI/CLI operation. A known
input extension constrains parsing; ASS tags inside SRT retain the SRT identity.
CLI output requires the same format extension. This operation does not convert
formats. Script headers, styles, comments, event fields, unknown records and
font attachment bytes remain unchanged; only accepted Dialogue cue times change.

A uniform movement preserves cue-relative effects. If the mapped cue duration
changes, karaoke (`\k`, `\K`, `\kf`, `\ko`, `\kt`), movement, transform and fade
tags require manual review, as do ambiguous overrides, nonempty event Effects
and cues over the 64KiB markup-analysis bound. Tags are never silently rewritten
or rescaled. This applies equally to ASS-tagged SRT. One unresolved cue blocks
whole-document acceptance and subtitle publication; the report remains available.
Static tags may keep their text while the cue duration changes. The policy is
conservative, including when format rounding alone changes a timed cue's duration.
The timing units are based on the [Aegisub tag reference](https://aegisub.org/docs/latest/ass_tags/).

## CLI workflow

Build without Tauri, a webview, libmpv or Node:

```sh
cargo build --locked -p vsf-cli
target/debug/vsf resync \
  --source-video original.mkv \
  --source-subtitles original.srt \
  --target-video destination.mkv \
  --output destination.srt \
  --report alignment.json
```

Windows uses `target/debug/vsf.exe`. Run `vsf resync --help` for options. All input
files must be local. `VSF_FFMPEG`/`VSF_FFPROBE` select explicit tools; the same
desktop development fallback applies otherwise. FFmpeg distribution/licensing
is not solved by this CLI. It never loads or modifies desktop settings/projects.

`--analysis-only` replaces `--output` and writes only the report. The report path
is always required; outputs must not already exist, even for a repeated run.
Unresolved cues produce a report and exit 3, with no subtitle output. The default total
time budget is 1800 seconds; `--timeout-seconds` accepts 1–14400. Ctrl-C requests
cancellation and waits for owned child cleanup; Unix SIGTERM/SIGHUP use the same
path. The publication commit boundary below still applies.

Progress and errors go to stderr. Normal operation leaves stdout empty; `--json`
prints one version-1 completion object containing `status`, `report`, `output`
(null if none) and `unresolved_cues`. Errors emit no completion JSON. A stdout
write failure after publication returns an IO error but retains published files.
Reports currently require Unicode-representable paths for JSON serialization;
encoding failure occurs before publication.

| Exit | Meaning |
| --- | --- |
| 0 | Complete report/subtitle publication, or complete analysis-only report. |
| 2 | Invalid arguments, subtitle/policy input or existing/aliased output. |
| 3 | Report published; some cues need review; no subtitles published. |
| 4 | Missing/failing analysis tools or decoding/matching failure. |
| 5 | Filesystem, signal-handler setup or completion-output IO failure. |
| 6 | Input content/identity changed after capture. |
| 7 | Total time limit exceeded. |
| 130 | Cancelled before publication began. |

```sh
cargo test --locked -p vsf-cli
cargo test --locked -p vsf-cli --test cli -- --ignored --nocapture --test-threads=1
```

The first command runs executable argument/failure checks and, on Unix, actual
SIGINT/timeout child-reaping tests with a controlled tool. The second requires
generated resync fixtures and runs actual FFmpeg through the executable. CI
configures a headless macOS/Windows/Linux matrix; no hosted execution is claimed.
FFmpeg comes from each runner's package manager and its exact version is recorded,
so the environment is not hermetically pinned. Windows runs the separate
`python tests/cli/windows-signals.py --vsf target/debug/vsf.exe` check after building
the CLI. It sends actual Ctrl-C in a private console and verifies cleanup of a
controlled child that ignores the event, plus timeout cleanup. This tests console
events, not physical keyboard input or console-window close/logoff behavior.
Clap 4.6.7 and ctrlc 3.5.2 are exact direct dependency pins; Cargo.lock pins their
transitive dependencies.

## Desktop workflow

1. Load the target video and the subtitles timed for the source version.
2. Choose **Tools → Resynchronize Subtitles…**, then select the source video.
   The dialog uses current accepted subtitle edits; resolve an unapplied cue
   draft with Apply or Discard, or cancel without changing it.
3. Analyze, then inspect matched sections and paged original/proposed timings.
   Missing/ambiguous cues prevent acceptance of the entire proposal.
4. **Accept timings** applies the complete proposal through the existing document
   owner. Undo/Redo changes all mapped timings together. It does not replace media.
5. Save As to a different file in the original format. Save Project first asks for that subtitle save;
   cancellation/failure leaves the project unwritten.

Closing an analysis cancels its job. During input verification, Cancel requests
cancellation; if Rust has already committed, its accepted document still wins.
The modal waits for a document mutation acknowledgment before closing. Candidate
subtitles are reviewed as timing rows; native video keeps accepted subtitles
until acceptance. Analysis does not open a second native player.

The browser [workflow fixture](../tests/frontend/resync/README.md) covers delayed
replies, drafts, review, acceptance, history and save/project sequencing with
modeled IPC. It is separate from Rust/FFmpeg and native execution evidence.

## Desktop proposal ownership

Four generated JSON commands start, inspect, cancel and accept one Rust-owned
proposal. Start captures the current accepted document and loaded target video;
the source video is read directly through FFmpeg. The frontend supplies a job ID
for acceptance, never computed cue timings. Review returns at most 100 cue rows
and 100 matched sections per request, including unresolved reasons and anchor
residuals. `ResyncSession` owns only the frontend projection and operation epochs;
`DocumentSession` serializes acceptance with other document commands.

`src-tauri/src/resync.rs` owns one analysis worker, with no pending queue. It
cancels and drains scene scanning before decoding; the existing scene generation
stays readable so frontend polling observes the stop. Completed scene caches and
partial markers remain available. New scans are rejected while resync analysis
or acceptance runs. Waveform may keep its one supervised child; both analyses
request two decoder threads and one filter thread, with decode passes sequential
inside each service. These are thread/process bounds, not a CPU-percentage limit.
Native playback remains independent.

Analysis and final input hashing run on background workers. Acceptance rechecks
media content identities, then locks media selection, resync ownership and the
document in that order. It checks job cancellation, media session and the full
document capture identity immediately before the single domain edit. Document
edits/Undo, replacement and Save As invalidate the old capture. Cancel remains
available during input verification. Shutdown closes admission, signals
cancellation, and waits for verification and worker cleanup off the UI thread.
Blocking filesystem calls can still delay cleanup.

Accepted retiming requires Save As to a different canonical path. That protection
survives Undo, no-op proposals and failed writes. Only a successful distinct save
clears it. Current recovery retains the flag, accepted source-format content and format identity; older SRT recovery versions remain readable. Rust also checks project subtitle references under the document
lock through publication, preventing a pending resync from attaching the original
subtitle file as though it contained the target timings. The frontend's save-before-project
continuation passes modeled-browser resync checks and the actual Mac native
walkthrough recorded below. Other OS native resync dialogs remain untested.

```sh
cargo test --locked -p visualsubforge
python3 scripts/resync-fixtures.py
cargo test --locked -p visualsubforge resync::tests::actual_analysis -- --ignored --nocapture --test-threads=1
```

The first command includes deterministic owner, cancellation, cleanup, document,
recovery and source-protection checks. The opt-in test runs actual FFmpeg through
the desktop owner, checks bounded review pages, acceptance and stale/changed-input
rejection, and uses a controlled media selection. It does not exercise native
playback, Tauri transport or GUI interactions. The desktop CI matrix configures
that test on all three OS runners; dated local results are recorded below.

## Shared application boundary

`crates/vsf-application` contains the desktop's existing atomic sibling writes,
file/save leases, source-format size policy and bounded subtitle file reads. The desktop imports
these implementations; their lifecycle tests run in the shared crate. Format parsing
and normalization validation use one facade. The domain and format crates
remain free of filesystem dependencies. The shared crate has no Tauri, webview,
native playback or frontend types.

These functions block and must run off the desktop UI thread. Document generation,
history, recovery coordination and project attachment remain with the existing
desktop owners. Extracting the demonstrated shared policies does not move those
owners or change existing save/lease semantics.

```sh
cargo test --locked -p vsf-application -p visualsubforge
cargo tree --locked -p vsf-application --edges normal
```

## Shared resynchronization operation

`vsf_application::resync::analyze` consumes either a subtitle file or an immutable
`SubtitleSnapshot` captured from accepted desktop state. It hashes source/target
media, records supervised FFmpeg/ffprobe version output, extracts scene anchors
and actual-PTS samples sequentially, matches/verifies supported intervals, and
builds an immutable `Proposal`. It never mutates accepted subtitles. The desktop
adapter retains job/media/document generation and revision guards.

The verifier requests dense target frames around unsupported intervals, in
two additional sequential passes. It compares the frame nearest the predicted
timestamp with alternative offsets; it does not slide a cue to the best-looking
nearby image. Verified extents retain an original cut-derived fit when merged,
so sampling cannot invent a new playback rate. Algorithm `scene-visual-edges-v4`
also observes the first and final decoded frames. An observed video edge can
bound a locally repeating first/last shot only when its timestamp agrees with
the cut model to 1 ms and the complete sampled interval supports that model.
Mismatched images, static content, missing samples or unmatched cuts prevent
that extension. Missing EOF evidence supplies no guessed boundary; equal
duration or matching edge pictures alone are insufficient. This does not locate
edits between different models. See the corpus record for measured limits.

The operation selects the first video stream and normalizes times to the
container origin. `Tools::discover` is shared with desktop scene/waveform work.
Development builds allow explicit `VSF_FFMPEG` and `VSF_FFPROBE` overrides and
system discovery. The `bundled-tools` release feature instead requires both
tools beside the executable and returns an installation error if either is
missing; it never falls back to environment overrides or system tools.
The algorithm version identifies fixed thresholds/configuration; tool version
text identifies the executed build, not a cryptographically pinned executable.

Each entire cue must fit inside one supported interval. Gaps, discontinuities,
reordered content and collapsed mapped ranges remain unresolved. The map must be
monotonic in both timelines; this preserves cue ordering and existing overlaps
without introducing inversions/overlaps between previously separated cues. No
sorting, text changes or minimum-duration repairs occur. Positive rational
arithmetic rounds directly to the source grid: 1ms SRT or 10ms ASS/SSA, nearest
with ties later. No intermediate whole-millisecond rounding precedes ASS/SSA ticks.

Every start and end maps independently from its original timestamp. Within a
supported section this scales both cue durations and gaps with the content; no
preceding end is moved to restore the old millisecond gap. For example, an actual
25→23.976 playback slowdown scales a 120 ms gap to approximately 125 ms and a
one-second pause to approximately 1.043 seconds. A conversion that merely drops
or duplicates frames without changing programme duration keeps the timing scale
at one. Matching establishes which case applies, including offsets after edits.

Each endpoint rounds once, so subtracting two rounded endpoints can differ by a
format tick from rounding an isolated gap. Never accumulate rounded gaps to find
later cues: that would introduce drift. A mapped cue that collapses to zero
milliseconds remains unresolved.

Neighbors are identified by source chronology without reordering the document.
Existing overlaps follow the same content map. Gaps across section boundaries
(ads/edits or unverified continuity) are reported separately, with no interpolation
or forced relationship to the original gap. Reports record the source gap and
its mapped size, or indicate different sections/an unresolved neighbor. Report
schema version 3 retains `content_mapped_endpoints_v1` from version 2 and adds
`subtitle_format`, `time_step_ms`, `endpoint_rounding` and `timed_effect_policy`.
`proposed_subtitle_sha256` replaces the SRT-only digest field. Schema 1's fixed
millisecond end correction remains superseded. The CLI completion-summary schema
remains version 1 because its shape is unchanged. Report import is not supported.

Minimum-gap and scene-buffer norms depend on the chosen profile; their values
cannot establish programme speed. Resync does not guess a profile from a gap or
promise full SW quality compliance. The separate reviewed minimum-gap editor action is documented in
[the quality workflow](subtitle-quality.md).

The complete candidate is validated through the existing batch domain operation
and source-format size policy. An incomplete proposal has no exportable subtitles.

One deadline and cancellation token cover input hashing, supervised subprocesses,
matching, proposal construction and final verification. At most one analysis child
is active; resync requests two decoder threads and one filter thread per decode
pass. Bounds include four-hour timelines, 256 GiB per media file, 20,000 cut
anchors, 30,000 coarse image samples per video and 30,000 additional target
frames, the existing pair-comparison caps, 16 MiB subtitle files and
64 MiB reports. Blocking filesystem operations can delay cancellation. Decoded
images remain inside the analysis library, never ordinary desktop IPC.
The final-frame observer uses two sequential passes per video, seeking five
seconds back from the container end (or decoding short clips from the start),
under the same deadline, cancellation and 30,000-frame/8 MiB metadata bounds.
It retains actual decoded PTS, not container duration or a nominal-FPS estimate.
A seek beyond the selected video's end, for example when audio is longer,
returns no edge evidence. No interval extends past its final observed frame PTS.
Dense filter expressions use temporary files to avoid platform command-line
limits. Their owner retains them until each supervised child has exited,
including cancellation/failure. FFmpeg 7+ uses its filter-file option; the older
syntax is selected for FFmpeg 6. That older branch is source-checked and unit
tested, but has not yet been exercised against a local FFmpeg 6 binary.

File identities include SHA-256 and canonical/requested paths. The subtitle file
is parsed from the same bytes that were hashed. Inputs are rehashed after analysis
and immediately before output publication. This detects ordinary changed inputs,
including same-size content edits; it is not filesystem snapshot isolation against
concurrent writers that change and restore bytes during reads.

`Analysis::publish` accepts new, distinct destinations only. It prepares and syncs
both files before verification, then publishes the versioned JSON report first
and subtitles last. An unresolved proposal publishes only its report and returns
`NeedsReview`. Analysis-only publication is also available. A report contains
input/config/tool identities, supported sections, per-cue outcomes and the proposed
subtitle digest; it never claims that the subtitles were published.

The existing persistence boundary uses `tempfile`'s
[`persist_noclobber`](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html#method.persist_noclobber)
for each new output, so a destination created after validation is not overwritten.
The report/subtitle pair is not an atomic transaction. Failure/crash after the report
can leave a report without subtitles; retry refuses the existing report. Ordinary
failure drops owned temporaries. Abrupt exit or platform cleanup limitations can
leave a temporary link. Cancellation immediately before the first publication
leaves outputs absent; after that commit point it does not roll back published
files. File data is synced during preparation; power-loss directory durability
is not claimed.

```sh
cargo test --locked -p vsf-application
python3 scripts/resync-fixtures.py
cargo test --locked -p vsf-application --test resync_ffmpeg -- --ignored --nocapture --test-threads=1
```

Pure tests cover independent endpoint rounding across generated speed scales,
short/long gaps, reverse speed conversion, unsorted source order, ad boundaries,
collapse refusal, stale snapshots, history, output collisions,
aliases, changed input hashes and mid-hash cancellation. A subprocess exits
between publications to verify crash/retry behavior. Actual FFmpeg integrations
cover offset, FPS-only conversion, speed/ad edits, unresolved cues, changed inputs
and cancellation. Both the CLI and desktop proposal owner call these operations.

## Atomic retiming in the subtitle domain

`subtitle_core::Edit::Retime` accepts one `CueTiming` for every existing cue, in
document order. Each entry contains a `CueId` and validated `Milliseconds` for
start/end. It cannot alter cue text, identities or ordering. Missing, duplicated,
unknown or reordered IDs fail before the document is changed. Existing overlaps
are allowed, just as with individual timing edits.

The application calls `Editor::apply_edit` with the captured revision and its
document validator. The editor validates every range, prepares the complete
change, runs the application policy against the proposed document, and then
publishes one revision/history entry. Policy rejection restores the original
document and preserves redo, saved-state identity and cue allocation. An
unchanged proposal creates no history entry. One Undo/Redo restores the whole
retiming while keeping source text intact.

This reuses the editor's existing range-change history; a whole-document retime
retains the before/after cue ranges. It does not introduce a second undo stack or
an asynchronous mutation path. The domain has no filesystem, Tauri, frontend DTO
or media dependencies. Analysis, proposal ownership, final timestamp rounding,
Save As protection and stale generation checks belong to the application layer.

Validate this boundary with:

```sh
cargo test --locked -p subtitle-core --test retiming
```

The tests include complete undo/redo restoration, invalid/stale/duplicate inputs,
policy rejection, no-op preservation and generated sequences of accepted edits.

## ASS/SSA extension validation — 2026-10-17

Task 020 extends the existing matcher; no thresholds, native strategy or analysis
limits change. Shared format tests cover direct rational/tick rounding, static
rate changes, all timed-tag families, unknown/incomplete/drawing overrides,
nonempty event Effects, oversized cues and whole-document refusal. Exact
styles/comments/unknown records/embedded-font bytes survive offset, serialization,
reopen and one Undo/Redo. Desktop owner checks cover style-change/Undo staleness,
failed saves, protected source paths and real checkpoint recovery for SRT, ASS,
SSA and embedded-font scripts.

Commands and evidence use Rust 1.98.1 and Node 24.21.0/npm 11.19.0. Node commands
follow `source "$HOME/.nvm/nvm.sh" && nvm use --silent`. Logs are retained under
`tests/fixtures/generated/styled-resync/` (ignored runtime evidence):

```sh
cargo test --locked --workspace
cargo test --locked -p vsf-application --test styled_proposal
cargo test --locked -p visualsubforge document::resync
cargo test --locked -p vsf-application --test resync_ffmpeg -- --ignored
cargo test --locked -p vsf-cli --test cli styled_and_tagged -- --ignored
cargo test --locked -p vsf-cli --test cli analysis_only_and_needs_review -- --ignored
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm run check
npm run lint
npm run format:check
npm test
npm run ipc:check
npm run test:browser
cargo build --locked --release -p vsf-application --example resync-benchmark
python3 scripts/resync-proposal-benchmark.py
```

The full Rust run passes 312 ordinary tests; the subsequent focused owner run
adds the style-only stale-capture test (313 ordinary tests in current source).
The four new shared tests and six owner tests are behavior checks, not zero-test
harnesses. Three opt-in FFmpeg application tests pass. Two actual-CLI tests pass,
including the 12-case ASS/SSA/tagged-SRT matrix: offset preservation, unsafe speed
refusal, static speed mapping and wrong-extension refusal with no output files.
The frontend's 44 unit tests and all 50 Chromium/WebKit browser cases pass, with
21 resync-specific checks per engine. Strict workspace/native-feature Clippy,
format, lint, Svelte/type/build and generated IPC checks pass.

Actual Mac native execution passes at `test-results/desktop-U0JjsN`. ASS/SSA and
ASS-tagged SRT each match the actual CLI output byte-for-byte, retain the source,
Undo/Redo together, preserve state after a failed save, acknowledge the accepted
preview revision and refuse whole-document acceptance on unsafe karaoke duration
changes. Webview review/refusal screenshots were inspected; the long dialog
scrolls at the tested window size. They are not native composition/GPU evidence.
The baseline real save/reopen/crash-recovery workflow and normal shutdown also pass.
The player reports Apple M3 Max OpenGL 4.1 using the existing private
`audio-integration-master-v1` runtime; this is not ANGLE/adoption or Windows/Linux
execution. Native binary SHA256 is
`aed2b86e54cc79b418f0065792f45ceeda8a8983777b9e6e43dc5c76db750439`;
CLI SHA256 is `9eeb9b5b652226aec31d24dd3c699cc2ac860a6ccbd6e9a3ab8c25d2528ffea9`.

```sh
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" npm run tauri -- build --debug --bundles app --features desktop-e2e,media-prototype --config "$PWD/tests/fixtures/generated/fonts/tauri.native.json" -- --locked
cargo build --locked -p vsf-cli
DYLD_LIBRARY_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build" "$HOME/.nvm/versions/node/v24.21.0/bin/node" tests/desktop/run.mjs --resync --binary target/debug/bundle/macos/VisualSubForge.app/Contents/MacOS/visualsubforge
PKG_CONFIG_PATH="$PWD/tests/fixtures/generated/audio-integration-master-v1/build/meson-uninstalled" cargo clippy --locked -p media-player -p visualsubforge --all-targets --features desktop-e2e,media-prototype -- -D warnings
```

Retained failures: the first exact-byte test incorrectly changed Comment timestamps
in its expected output; the implementation preserved them. Native runs
`desktop-42mORz`, `desktop-E5UCEo`, `desktop-pzApiP`, `desktop-S6QhBK` and
`desktop-8fGOF2` exposed harness assumptions: picker paths must be test-local,
backend readiness precedes the frontend's loaded-media projection, rendered text
contains line breaks, and save failures appear in the status region. The final
harness waits for enabled controls/command acknowledgment and asserts the actual
save error. No product safety bound or timing tolerance was relaxed.

The predeclared benchmark retains 10,000 cues and all text/metadata/opaque records.
Task 016's original fixture spans 4h10m, above the existing 4h resync limit; the
explicit timing-only derivative divides endpoints by 2, preserving the 10ms grid.
The benchmark records both parent and derived hashes and leaves originals intact.
Proposal/snapshot/acceptance/serialization/Undo p95 is 103.088ms ASS and 97.135ms SSA,
with 32,931,840 bytes additional sampled RSS. This passes the unchanged 500ms/96MiB
budgets. RSS sampling is not exhaustive allocation accounting; this is application
cost evidence and does not measure video analysis, physical input or GPU output.

## Recorded resync validation

The entries below retain incremental results, including failures and then-open
gates. Later v4 and native-walkthrough results supersede those earlier pending
states; the opening summary describes current acceptance.

The 2026-10-11 VM checks used guest-local copies of committed source `d80580e`,
with Rust 1.98.1. The identical source archive SHA-256 was
`81c61292d754634defb791e6ae33a4ef82812e18bc3cba80cfd2b33e94c50ef8`.
Files and advisory locks were exercised on each guest's local filesystem, not
a shared host folder. The Windows console harness was then added and its source
hashes verified before execution. These are ARM64 machines, not the x64 hosted
runner images. No Git remote is configured, so hosted CI remains unrun.

| Environment | Completed checks and scope |
| --- | --- |
| Development macOS ARM64; FFmpeg 9.0.2 | Earlier full workspace, CLI/shared-operation FFmpeg tests, 16 extended/reference alignment cases, and 30 modeled Chromium/WebKit workflow cases pass. GUI native build/preflight and loaded-library recording pass; real resync interaction is pending. |
| Ubuntu 26.04 ARM64; FFmpeg 8.0.1-3ubuntu2; Python 3.14.4 | 62 headless Rust tests, three shared-operation, two executable and three visual FFmpeg integrations pass. Ten compact alignment cases pass. Desktop: 93 ordinary tests, the explicit FFmpeg proposal-owner test, generated IPC drift check, 39 frontend tests, production build and strict workspace Clippy pass. |
| Windows 11 ARM64, build 22631.6199; MSYS2 FFmpeg 9.0.2; Python 3.14.7 | 60 headless Rust tests and the same eight FFmpeg integrations pass. Ten compact alignment cases pass. Actual Ctrl-C returns 130 in 0.092 seconds; the two-second timeout returns 7 in 2.071 seconds. Both terminate the controlled child and preserve inputs without publishing outputs. Desktop: 88 ordinary tests, explicit FFmpeg proposal-owner test, IPC drift, 39 frontend tests, production build and strict workspace Clippy pass. |

Both VM compact-corpus runs measure 93.7–100% interior coverage in positive cases,
zero measured accepted timing error, abstention on unrelated/repeated/flat footage,
and an explicit reordering diagnosis. The extended/reference corpus has not been
rerun in these VMs. The Ubuntu fixture interpreter differs from CI's Python
3.14.7 pin; its actual version is recorded rather than claiming an identical
environment. Desktop frontend checks use Node 24.21.0 and npm 11.19.0.

Reproduce headless checks using the CLI/shared-operation commands above and
the [visual fixture/evaluation commands](../tests/fixtures/resync/README.md).
The default Rust scope was `cargo test --locked -p subtitle-core -p
subtitle-formats -p media-analysis -p vsf-application -p vsf-cli`; strict Clippy
used the same headless application/analysis/CLI packages. Desktop commands are
in [CONTRIBUTING](../CONTRIBUTING.md), with the opt-in proposal-owner test above.
Zero-test harnesses and deliberately ignored integrations are excluded from
these counts. The Windows count excludes Unix-only signal and alias controls;
the separate Windows console check closes the signal-delivery gap.

Ignored receipts, exact command logs, truth and evaluation results are under
`tests/fixtures/generated/resync/platform-validation-v1/`. The initial Mac native
build, preflight and mapped-file receipts use the `gui-native-*-v1` prefix in
the parent directory. They do not establish GPU/audio quality or completed GUI
resync acceptance. Subsequent Mac v2 matching checks add fractional rates,
negative PTS, fades/crop/logo/brightness controls and a second reference film.
The live-action speed/ad edit passes at 95.8% coverage and 5.385 ms worst accepted
error; an actual three-cue CLI run preserves inputs and publishes the new SRT.
Two new low-cut cases still miss the 90% coverage gate (85.3% and 85.6%), with
zero measured accepted error; brightness changes abstain. The
[corpus record](../tests/fixtures/resync/README.md#additional-robustness-and-live-action-evaluation)
retains the failures and exact reproduction commands. These newer cases and
the v2 guard have not been rerun in the VMs. Actual native review/Undo/Save As/
project interaction, x64 and hosted CI remain open.

Subsequent v3 Mac validation: 162 ordinary workspace tests, strict workspace
Clippy, four visual, three shared-operation, two executable and one desktop-owner
FFmpeg integrations pass. Original 16-case and 11-case live-action corpora pass;
live-action coverage is 96.6%. The expanded robustness corpus passes 20/21 cases:
periodic-motion coverage remains 77.8%, below its unchanged 90% gate. A new
repeating-tail deletion that v2 accepted with a 1,760 ms error is now unresolved.
An intermediate live-action regression was a merge defect, fixed by retaining
the original cut model rather than blending endpoints. These are correctness
and coverage measurements, not GPU/audio or native GUI acceptance. Retained
corpus receipts use `evaluation-v3-preserved-models.json`.

V3 checks at `f3df34c` also pass in the supplied VMs (same Rust/FFmpeg versions
as the table above): Ubuntu ARM64 runs 164 ordinary workspace tests and Windows
ARM64 runs 157. Each runs all ten opt-in visual/application/CLI/desktop-owner
FFmpeg integrations, all 16 current extended synthetic cases, and strict
workspace Clippy. SHA-256 checks match every one of the 368 tracked source files.
Receipts, exact commands, truth and results are in ignored
`tests/fixtures/generated/resync/platform-validation-v3/`. Both include the
filter-file expression larger than Windows' command-line limit and rejection
of the repeating-tail deletion. Live-action and robustness corpora remain Mac
evidence. CI now configures visual sampling and compact evaluation on all three
headless OS runners; hosted execution remains unverified.

An actual Mac v3 CLI live-action run preserves all input hashes and reproduces
the previously verified three-cue output (maximum rounded endpoint error
5.552 ms). A separate run receives SIGINT while FFmpeg reads a dense filter file:
exit 130 in 0.007 seconds, the observed child reaped, both temporary filters
removed, unchanged inputs and no report/SRT output. The retained
`resync-live-action-v1/cli-*-v3` receipts distinguish these checks from GUI or
audible synchronization. Windows was restored to its initial paused state with
idle pausing enabled; Ubuntu was left running as found.


V4 Mac checks (2026-10-11): `cargo test --locked --workspace` passes 165 ordinary
tests; 26 opt-in tests remain ignored in that command. Strict workspace Clippy,
rustfmt, workflow Prettier and all three evaluator-oracle tests pass. Separately,
the documented commands run six visual/sampling, three application, two CLI
and one desktop-owner FFmpeg integrations: twelve pass. New EOF tests cover
short video, positive origins and audio that outlasts video. Exact command output
is retained in `tests/fixtures/generated/resync/validation-v4-*.log`.
All 29 combined extended/robustness cases and both reference-film corpora pass;
see the corpus record for coverage, timing errors and abstention controls.
The v4 source at `921181c` also passes on Ubuntu 26.04 ARM64 (167 ordinary
tests) and Windows 11 ARM64 (160). Both run all twelve integrations, three
evaluator-oracle tests, the 29-case combined corpus and strict workspace Clippy.
The complete 369-file source manifest matches before execution. The first
transfer retained epoch timestamps and caused stale Cargo reuse; refreshing
changed-file mtimes fixed that harness issue. A separate driver output-name
collision stopped before evaluation; only the remaining evaluation/Clippy steps
were resumed after verifying the terminal process and unchanged source. Failed
receipts are preserved, not counted as passing product checks.

Exact completed commands are in ignored `resync/platform-validation-v4-fresh/`;
its completed checks are linked by receipt SHA-256 from
`resync/platform-validation-v4-completion/`, which holds final evaluation/Clippy
results. Original harness failures remain in `resync/platform-validation-v4/`.
Windows was restored to paused with idle pausing on; Ubuntu stayed running.
The selected Mac native preflight/no-bundle build and ad-hoc local bundle
verification also pass at `ddc841a`. The current prepared bundle is
`/tmp/vsf-resync-ddc841a-native/VisualSubForge.app`, with receipts in ignored
`resync/gui-native-ddc841a/`. The earlier bundles are preserved. These are build
checks; the subsequent native interaction evidence is recorded below.

## Native resync walkthrough

On 2026-10-11, the previously locked Mac became available. The prior empty app
was closed normally, then the `ddc841a` bundle ran with its selected private
mpv/ANGLE libraries. No `desktop-e2e` feature, supplied chooser, or mocked command
was used. Actual native menus and file dialogs exercised this sequence:

1. Open generated `offset.mkv` as the target and a source SRT with labels 7/8/9,
   Unicode text, a 100 ms gap and an 11-second pause. Select `source.mkv` in Tools
   → Resynchronize Subtitles and run analysis.
2. Review all three old/new cue ranges and the matched section: scale 1,
   offset +3 seconds, 17 anchors and zero maximum fit error. Accept once; Undo
   once restores all source ranges and Redo once restores all proposed ranges.
3. Verify that the Save As requirement survives Undo. Cancel Save As, then
   explicitly choose the original SRT and confirm the OS replacement prompt.
   VSF refuses that path and retains the accepted timings; the source hash is
   unchanged.
4. Invoke Save Project before saving subtitles. Its continuation first saves a
   distinct `resynced.srt`, then `resynced.vsfproject`. Close both subtitles and
   media; reopen the project and verify all timings, the target media and paused
   playback. Quit normally and verify the owned process has exited.

The output is byte-for-byte the expected SRT: 6–7 s, 7.100–8 s and 19–20 s,
with original labels/text retained. The project references `resynced.srt`, not
the source; all three input hashes are unchanged. The run’s executable SHA-256
is `8c7c60ecc07e41abfa6afa63faa3050d936aeeaa6d5b453c3bc3cfdf81fd736a`.
`scripts/record-native-runtime.py` confirmed the executable, libmpv and both
ANGLE images actually loaded. The visible native surface and renderer log show
ANGLE/Metal on the M3 Max. This is a video-only fixture; “No mapped audio track”
is expected and supplies no acoustic or audio-synchronization evidence.

Ignored evidence is retained in
`tests/fixtures/generated/resync/gui-native-walkthrough-v1/`: input/binary hashes,
loaded-library receipt, accessibility states, proposal/reopen screenshots, exact
output files and `verification.json` recording all eleven checks and normal quit.
This controlled native offset workflow complements the separate automated
speed/ad/trim corpus and three-OS default-app lane. It does not claim native
resync interaction on Windows/Linux or product-wide GPU/audio acceptance.

## Actual default-app workflow and IO-failure checks

The WebdriverIO route was evaluated against the current
[Tauri guide](https://v2.tauri.app/develop/tests/webdriver/) and
[plugin setup](https://webdriver.io/docs/desktop-testing/tauri/plugin-setup).
The published embedded plugin `tauri-plugin-wdio-webdriver` 1.5.0 supplies a
loopback WebDriver server and standard script/element operations; the broader
Tauri service adds separate plugins for logging/backend helpers and mocking.
The [implemented lane](../tests/desktop/README.md) uses only the embedded plugin,
pinned at 1.5.0 behind `desktop-e2e`, with WebdriverIO 10.0.2. It excludes the
broader helper/mocking plugins and leaves document IPC unmocked. Test app identity,
settings and recovery are isolated; ordinary builds exclude the driver. File
selection is supplied by a bounded queue. Mac/Ubuntu/Windows strict feature Clippy passes;
the instrumented executable rejects a missing test root before app startup, and
the normal dependency tree excludes the driver. The first workflow runs exposed
an incorrect test expectation about ordinary recovery: explicit Save As can reuse
an unchanged source. The corrected case simulates a newer external edit and checks
that recovery preserves it. Failed-run evidence is retained. Resync's distinct-file
rule is separate and remains tested in Rust.

Run `npm run build:desktop-test`, then `npm run test:desktop` with the README's
Xvfb/session-bus wrapper on headless Linux. Receipts/logs/screenshots are retained
under `test-results/desktop-*`; local completed evidence is copied into ignored
`resync/desktop-e2e-{mac,linux}-v3/` and `resync/desktop-e2e-windows-v2/`.
All three local ARM64 workflows pass actual IPC, edits/Undo/Redo, failed/cancelled
saves, exact-byte save/reopen, process-loss recovery, external-file conflict
protection and normal shutdown. The Windows first-run path-prefix mismatch is
retained as a test failure; normalizing the representation fixes the assertion
without changing product persistence. The updated client and source-map patch
pass a dated zero-advisory npm audit; details are in the lane README.
CI configures all three desktop OSes.
Virtual-display execution does not establish native GPU/audio performance.
Hosted/x64 CI still requires an
available remote/runner; no such execution is claimed here.


Preparation IO-failure validation (2026-10-11): a private `prepare_with_io` seam
substitutes only the write and sync operations on an actual temporary file.
The public persistence API and publication sequence are unchanged. Four cases
force either a partial write error or a sync error after a complete write,
against existing and absent destinations. They assert original bytes/absence,
error propagation, no sync after a failed write and removal of owned temporaries.
This tests preparation failure handling, not disk hardware or power-loss recovery.
`cargo test --locked -p vsf-application -p visualsubforge` passes 111 ordinary
tests; strict Clippy for both packages passes on Mac. Logs are retained as
`resync/validation-io-failures*.log`. Ubuntu and Windows ARM64 also pass both
preparation tests via `cargo test --locked -p vsf-application atomic_file::tests`,
recorded in `resync/desktop-e2e-linux-v2/` and `resync/desktop-e2e-windows-v2/`.
Filtered zero-test integration harnesses are not additional evidence. The Windows
test-owned process handles were confirmed terminal before restoring idle pausing
and the VM's original paused state.

## Historical status receipt: ASS/SSA resync complete (2026-10-17)

Relocated from the status page. This receipt retains the scope and pending
work at that checkpoint; subsequent evidence and the [current status](status.md)
supersede its staging statements. Moving it adds no validation claim.

**ASS/SSA resync complete (2026-10-17):** shared CLI/GUI analysis preserves source
styles, metadata, attachments and tags. Rational endpoints round directly to the
stated source grid. Cues with timed/ambiguous effects can move intact; duration
changes require manual review and block whole-document acceptance, including
ASS tags inside SRT. Accepted batches retain source protection and one-step Undo.

Stage3 validation: 313 ordinary Rust tests, 44 frontend tests, 50 browser cases,
real FFmpeg/CLI matrices and strict static/build/IPC checks pass. Actual Mac
ASS/SSA/tagged-SRT review, acceptance, exact CLI/save-byte parity, failed-save
preservation, Undo/Redo, preview acknowledgment and timed-effect refusal pass.
10k proposal p95 is <=103.088ms with 32,931,840 additional sampled RSS bytes,
within 500ms/96MiB budgets. See [policy and exact evidence](resynchronization.md#assssa-extension-validation--2026-10-17).
Windows/Linux new-feature execution and physical installation/GPU/audio acceptance
remain separate open gates; the approved native runtime strategy is unchanged.

## Historical matcher progression

### v4 and preceding v3/v2 evidence

Algorithm `scene-visual-edges-v4` adds actual first/final decoded frame evidence
for otherwise ambiguous first/last shots, while preserving the cut-derived fit.
Every intervening observation must support it; matching edge pictures or equal
duration alone do not establish continuity. On the development Mac, the combined
29-case extended/robustness corpus passes. Periodic-motion coverage rises from
77.8% to 93.3%, above the unchanged 90% target, with zero measured accepted error.
Whole-cycle deletions in the first/last shot remain unresolved; same-duration
120 ms replacements do not get bridged. Both reference-film corpora still pass
(animated coverage 97.9%; live action 96.6%, worst accepted endpoint error 5.385 ms).
The 165 ordinary workspace tests, strict Clippy and twelve explicitly invoked
FFmpeg integrations pass. The new EOF tests cover short clips, nonzero origins
and a longer audio track that supplies no video-edge evidence. At `921181c`,
Ubuntu/Windows ARM64 pass 167/160 ordinary tests, the same twelve integrations,
all 29 combined cases, three evaluator-oracle tests and strict Clippy. All 369
tracked files are hash-verified. The `ddc841a` Mac native bundle subsequently
passed proposal review/acceptance, one Undo/Redo, cancelled Save As, explicit
source-path rejection, save-before-project and reopen from an empty workspace.
Exact output bytes and all input hashes match expectations; the project refers
to the new SRT. The app quit normally. The [native walkthrough](resynchronization.md#native-resync-walkthrough)
records executable/runtime identities and retained screenshots. This closes
the first resync milestone; video-only fixtures do not validate audio. The
[real desktop workflow lane](../tests/desktop/README.md) now passes on Mac,
Ubuntu (Xvfb) and Windows ARM64, with actual webview/IPC, editing/history, failed/cancelled
saves, exact-byte reopen, process-loss recovery, external-file conflict protection
and normal shutdown. Its optional pinned WebDriver instrumentation is absent from
ordinary builds; only file choices and app-data location are supplied. CI
now also passes this lane on all three hosted desktop OSes at `a222eda`; see
[release evidence](releases.md) for run links and packaging limits.
The pinned test client is now WebdriverIO 10.0.2, and source-map-js is patched to
1.2.2; a dated npm audit reports zero advisories. A new
private IO seam now tests partial-write and sync failures on actual temporary
files, preserving existing/absent destinations and cleaning temporaries; the
111 application/desktop tests and strict Clippy pass on Mac; both preparation
tests also pass on Ubuntu and Windows ARM64.
These sampled heuristics
do not guarantee detection of visually indistinguishable edits.

The following results describe the preceding committed v3 implementation:

Algorithm `scene-visual-neighborhoods-v3` adds bounded dense target
observations around unsupported intervals. A newly generated deletion of one
repeating motion cycle exposed a 1,760 ms false accepted mapping in v2; v3 leaves
that tail unresolved. All original 16 cases pass, with reference coverage 97.9%.
The expanded 21-case robustness corpus passes 20 cases: the nonrepeating pan
reaches 92.9% and its tail-deletion variant 95.7%, but periodic-motion coverage
remains below its 90% gate at 77.8%. These cases have no measured wrong accepted
timings. Live-action coverage reaches 96.6%, with a 5.385 ms worst accepted
endpoint error. An intermediate 80.9% regression exposed an interval merge that
mixed two fits' endpoints and invented a third slope. Retaining an original
cut model fixes the regression; its failing unit test and corpus evidence are
preserved. Latest local workspace tests (162), strict Clippy and ten explicitly
invoked visual/shared-operation/CLI/desktop-owner FFmpeg tests pass. At `f3df34c`,
Ubuntu ARM64 passes 164 ordinary workspace tests and Windows ARM64 passes 157;
both pass the same ten FFmpeg integrations, all 16 regenerated extended cases
and strict Clippy. Source hashes match all 368 tracked files. The Windows
filter-file test exceeds its command-line limit without putting the filter on
that command line. Mac Ctrl-C during actual dense sampling reaps the child,
removes both filter files and publishes nothing. Native GUI validation remains.

The following results describe the preceding committed v2 implementation:

The sampled verifier now preserves cut-derived evidence on either side of
differing timing models and leaves their uncertain gap unresolved. The original
16-case corpus still passes, with reference coverage 94.3% and zero measured
accepted error. A new low-cut/short-deletion experiment improves from 50.6% to
85.3% coverage but still misses its 90% target. A separate nonrepeating pan
reaches 85.6%; inspection shows that half-second sampling can miss the matching
frame at the predicted timestamp. The held-out live-action speed/ad edit reaches
95.8% coverage with 5.385 ms worst accepted error, and an actual three-cue CLI
run preserves inputs and publishes correct output. Matching quality remains an
open gate; see the [corpus record](../tests/fixtures/resync/README.md).

The [dated resync validation record](resynchronization.md#recorded-resync-validation)
distinguishes the current macOS, Ubuntu ARM64 and Windows ARM64 results. Both VM
headless suites, actual FFmpeg operations and ten compact alignment cases pass.
Both VMs' desktop owner/persistence, generated IPC and frontend checks pass.
Windows console Ctrl-C and timeout terminate the analysis child and preserve
files. Native GUI resync and the broader matching corpus were still open at
that earlier stage; the v4 and native-walkthrough results above supersede it.

## Historical application and CLI increments

These are preceding increments, not current open gates.

Subsequent resync experiment checks on the development Mac: 23 media-analysis
unit tests and strict Clippy passed, plus all three visual integrations. The
four waveform and seven scene integrations passed in the preceding increment.
All 16 extended/reference corpus cases pass: reference coverage rose from 82.8%
to 95.2% through sampled verification, held-out speed/ad coverage is 99.2%, and
the long-block case reaches 100%. Accepted intervals have zero measured timing
error; static-deletion tails remain unresolved. These results do not establish
general content/distortion quality or other-platform behavior.

Shared extraction: `cargo test --locked -p vsf-application -p visualsubforge`
passed 73 tests (70 desktop, three moved persistence/lease tests); the save-lock
helper is invoked by its owner. Strict Clippy for both packages and rustfmt passed.
This preserves existing behavior; it does not validate a product CLI yet.

Shared resync operations: 15 application tests and all 70 desktop tests passed,
including process-crash/report-only publication, collision preservation, rounding,
hash changes and cancellation. Three additional actual-FFmpeg integrations passed
(offset/FPS/speed-ad mapping, unresolved output, changed inputs and cancellation).
The 23 media-analysis tests and strict Clippy for all three affected packages
passed. Publication behavior is locally verified on macOS; OS CI execution and
the product adapters remain outstanding.

CLI executable checks on macOS: three default tests and two opt-in FFmpeg tests
passed, including JSON stdout separation, report/SRT round trips, unresolved
output refusal, retry protection, actual SIGINT cancellation and active-child
reaping on cancellation/timeout. Strict CLI Clippy passed. A headless OS matrix
is configured but not run here; Windows console signals remain unverified.

Historical exact-gap refinement (superseded by the content-timing correction below): eight proposal tests passed, including 32-cue sequences at
46 speed scales, unsorted SRT order, Undo/Redo, ad-boundary exceptions and collapse
refusal. Three application FFmpeg tests and two CLI FFmpeg tests pass with an
actual 100 ms source gap preserved at 24/25 speed and a separate ad gap left to
the section mapping. These validated the then-approved end adjustment; current resync instead maps
every endpoint directly, as documented below.

After the CLI and exact-gap changes, `cargo test --locked --workspace` passed
131 tests and strict workspace Clippy passed. The workspace command leaves
FFmpeg integrations opt-in; the three shared-operation and two CLI FFmpeg tests
were run separately and passed. Formatting and diff checks passed. This is
macOS default-build evidence, with no new native GPU/audio or other-OS claim.

Content-timing correction (2026-10-11): the user clarified that every cue start
and end, including long pauses, should follow actual matched programme timing.
The superseded fixed-ms end adjustment is removed. Reports use schema 2 and
`content_mapped_endpoints_v1`, with original/resulting gaps and separate section
boundaries. FPS-only conversion is not treated as a speed change. Preset/minimum
gap enforcement was a separate follow-up (now implemented in the quality milestone);
audio matching remains deferred.

`cargo test --locked -p vsf-application -p vsf-cli` passed 22 tests, including
nine proposal tests. Generated sequences cover 256 cues at 46 scales with
independent endpoint expectations, short/long gaps, plus explicit 25→23.976
speed examples and reverse mapping. An initial round-trip assertion compared
un-normalized SRT trailing whitespace; it now compares parsed documents.
All three shared-operation and two executable FFmpeg tests passed via the
resync guide's opt-in commands. Strict application/CLI Clippy, rustfmt and diff
checks passed. This changes proposal timing/report policy, not visual matching;
the corpus need not be retuned. GUI integration and remaining platform/content
gates stay active.

## Historical desktop resync increments

These October 11 increments precede the native walkthrough above. Their pending
gates and absent hosted results describe that date, not current status.

Desktop resync backend validation (2026-10-11, development Mac, pinned Rust/Node,
FFmpeg 9.0.2): `cargo test --locked --workspace` passed 153 tests, including
91 desktop tests; zero-test harnesses are not feature evidence. The separately
invoked `cargo test --locked -p visualsubforge resync::tests::actual_analysis --
--ignored --nocapture --test-threads=1` passed, exercising actual FFmpeg, bounded
review pages, acceptance and changed-input rejection with controlled media
selection. `cargo test --locked -p media-analysis --test ffmpeg -- --ignored
--nocapture --test-threads=1` passed all four waveform integrations after bounding
its decode threads (two-hour fixture: 4.126 seconds). Strict Clippy for desktop
and media-analysis, Rust formatting, frontend build, IPC drift, lint, 39 frontend
unit tests and formatting passed. `cargo check --locked -p visualsubforge
--features media-prototype` passed; this is compilation, not native execution.

New deterministic tests cover drain/reuse/panic handling, no queued resync job,
cancellation and shutdown ownership, media-selection serialization, stale
captures, one-edit Undo/Redo, protected Save As, actual recovery publication,
legacy recovery reads, project-reference rejection and retained scene polling.
The CI desktop matrix now configures the actual-FFmpeg owner test on all three
OSes. No remote is configured, so no hosted run is claimed. GUI interaction,
real Tauri transport, wider corpus and remaining OS checks are still required.

GUI review increment (2026-10-11): Tools opens a focused resync dialog and
`ResyncSession`, with source/target roles, draft apply/discard/cancel, progress,
100-row cue/section pages, unresolved reasons and complete-proposal acceptance.
DocumentSession uses one serialized reconciliation path for ordinary actions
and resync acceptance. App adds modal occlusion, focus and dispatch only. Closing
while Start is pending cancels its eventual ID; old view/picker replies cannot
revive a disposed job. The dialog waits for draft/accept acknowledgment before
allowing Close, preventing late accepted replies from overwriting later typing.

`npm run test:browser` passed all 30 Chromium/WebKit cases, including the new
resync workflow. After adding explicit draft-discard and same-revision Save As
controls, `npm run test:browser -- --grep 'workflow: resync'` passed all 21 fixture
assertions on each engine. `npm run build`/`npm run check` passed without warnings;
IPC generation/drift, lint and formatting passed. Strict desktop Clippy and
`cargo test --locked -p visualsubforge` passed all 91 ordinary tests (explicit
exporter/FFmpeg tests remain opt-in). No matching algorithm changed.

Interactive modeled review was inspected at 1440×1000 and 480×700. The narrow
dialog stayed inside the viewport without outer horizontal overflow; tables
scroll separately, including PageDown after keyboard focus. Ignored screenshots
are `tests/fixtures/generated/resync/gui-review-{wide,narrow}.png`. These browser
checks model media, file dialogs and storage. Actual Tauri/native resync, remaining
platform/process/filesystem and broader corpus validation remain open.
