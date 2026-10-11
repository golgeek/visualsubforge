# Subtitle resynchronization implementation

The first release is under development. The current alignment experiment is
documented in the [corpus guide](../tests/fixtures/resync/README.md). There is no
product CLI or desktop resync interface yet.

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

Pure tests cover rounding, gaps, stale snapshots, history, output collisions,
aliases, changed input hashes and mid-hash cancellation. A subprocess exits
between publications to verify crash/retry behavior. Actual FFmpeg integrations
cover offset, FPS-only conversion, speed/ad edits, unresolved cues, changed inputs
and cancellation. These are application operations; product CLI and GUI adapters
are the next increments.

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
