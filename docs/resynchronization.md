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
Unresolved cues produce a report and exit 3, with no SRT. The default total
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
| 0 | Complete report/SRT publication, or complete analysis-only report. |
| 2 | Invalid arguments, subtitle/policy input or existing/aliased output. |
| 3 | Report published; some cues need review; no SRT published. |
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
5. Save As to a different SRT. Save Project first asks for that subtitle save;
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
clears it. Recovery format 3 retains the flag and accepted SRT; versions 1 and 2
remain readable. Rust also checks project subtitle references under the document
lock through publication, preventing a pending resync from attaching the original
SRT as though it contained the target timings. The frontend's save-before-project
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
file/save leases, SRT size policy and bounded SRT file reads. The desktop imports
these implementations; their lifecycle tests run in the shared crate. SRT parsing
and normalization validation use one entry point. The domain and format crates
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
container origin. `Tools::discover` uses the same explicit `VSF_FFMPEG` and
`VSF_FFPROBE` overrides and development fallback as desktop scene/waveform work.
The algorithm version identifies fixed thresholds/configuration; tool version
text identifies the executed build, not a cryptographically pinned executable.

Each entire cue must fit inside one supported interval. Gaps, discontinuities,
reordered content and collapsed mapped ranges remain unresolved. The map must be
monotonic in both timelines; this preserves cue ordering and existing overlaps
without introducing inversions/overlaps between previously separated cues. No
sorting, text changes or minimum-duration repairs occur. Positive rational
arithmetic rounds once to milliseconds, with half-millisecond ties rounded up.

Every start and end maps independently from its original timestamp. Within a
supported section this scales both cue durations and gaps with the content; no
preceding end is moved to restore the old millisecond gap. For example, an actual
25→23.976 playback slowdown scales a 120 ms gap to approximately 125 ms and a
one-second pause to approximately 1.043 seconds. A conversion that merely drops
or duplicates frames without changing programme duration keeps the timing scale
at one. Matching establishes which case applies, including offsets after edits.

Each endpoint rounds once, so subtracting two rounded endpoints can differ by a
millisecond from rounding an isolated gap. Never accumulate rounded gaps to find
later cues: that would introduce drift. A mapped cue that collapses to zero
milliseconds remains unresolved.

Neighbors are identified by source chronology without reordering the document.
Existing overlaps follow the same content map. Gaps across section boundaries
(ads/edits or unverified continuity) are reported separately, with no interpolation
or forced relationship to the original gap. Reports record the source gap and
its mapped size, or indicate different sections/an unresolved neighbor. Report
schema version 2 uses `content_mapped_endpoints_v1`; it replaces schema 1's
superseded fixed-millisecond end correction. The CLI completion-summary schema
remains version 1 because its shape is unchanged. Report import is not supported.

Minimum-gap and scene-buffer norms depend on the chosen profile; their values
cannot establish programme speed. Resync does not guess a profile from a gap or
promise full SW quality compliance. The separate bulk minimum-gap editor action
is not implemented.

The complete candidate is validated through the existing batch domain operation
and SRT size policy. An incomplete proposal has no exportable SRT.

One deadline and cancellation token cover input hashing, supervised subprocesses,
matching, proposal construction and final verification. At most one analysis child
is active; resync requests two decoder threads and one filter thread per decode
pass. Bounds include four-hour timelines, 256 GiB per media file, 20,000 cut
anchors, 30,000 coarse image samples per video and 30,000 additional target
frames, the existing pair-comparison caps, 16 MiB SRT and
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
and SRT last. An unresolved proposal publishes only its report and returns
`NeedsReview`. Analysis-only publication is also available. A report contains
input/config/tool identities, supported sections, per-cue outcomes and the proposed
SRT digest; it never claims that the SRT was published.

The existing persistence boundary uses `tempfile`'s
[`persist_noclobber`](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html#method.persist_noclobber)
for each new output, so a destination created after validation is not overwritten.
The report/SRT pair is not an atomic transaction. Failure/crash after the report
can leave a report without an SRT; retry refuses the existing report. Ordinary
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
