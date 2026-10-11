# Subtitle resynchronization implementation

The first release is under development. The current alignment experiment is
documented in the [corpus guide](../tests/fixtures/resync/README.md). A headless CLI
now calls the shared operation. The desktop backend owns proposal review pages,
acceptance and persistence protection; its GUI is still being connected. Wider
matching/platform validation remains open.

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
so the environment is not hermetically pinned. Windows console-event delivery
still needs direct evidence; portable cancellation-token tests do not prove it.
Clap 4.6.7 and ctrlc 3.5.2 are exact direct dependency pins; Cargo.lock pins their
transitive dependencies.

## Desktop proposal ownership

Four generated JSON commands start, inspect, cancel and accept one Rust-owned
proposal. Start captures the current accepted document and loaded target video;
the source video is read directly through FFmpeg. The frontend supplies a job ID
for acceptance, never computed cue timings. Review returns at most 100 cue rows
and 100 matched sections per request, including unresolved reasons and anchor
residuals. The GUI controls are not implemented yet.

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
continuation still needs resync workflow validation.

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
that test on all three OS runners; local execution is currently macOS only.

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
adapter must still retain job/media/document generation and revision guards.

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
and cancellation. The CLI calls these operations directly; GUI integration is next.

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
