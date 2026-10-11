# Subtitle resynchronization implementation

The first release is under development. The current alignment experiment is
documented in the [corpus guide](../tests/fixtures/resync/README.md). A headless CLI
now calls the shared operation. The desktop backend owns proposal review pages,
acceptance and persistence protection, with a Svelte review workflow under Tools.
Wider matching/platform and actual-native workflow validation remain open.

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
continuation passes modeled-browser resync checks; actual native interaction
remains a separate acceptance gate.

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
anchors, 30,000 image samples, the existing pair-comparison caps, 16 MiB SRT and
64 MiB reports. Blocking filesystem operations can delay cancellation. Decoded
images remain inside the analysis library, never ordinary desktop IPC.

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
`tests/fixtures/generated/resync/platform-validation-v1/`. The current Mac native
build, preflight and mapped-file receipts use the `gui-native-*-v1` prefix in
the parent directory. They do not establish GPU/audio quality or completed GUI
resync acceptance. Wider held-out content, fades/crop/logo robustness, actual
native review/Undo/Save As/project interaction, x64 and hosted CI remain open.
