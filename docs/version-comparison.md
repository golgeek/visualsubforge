# Version comparison

The headless comparison engine compares immutable original and revised subtitle
captures without changing either file. Desktop review attaches an independent
original to the ordinary editable document.

```sh
vsf compare original.srt revised.srt --json --html comparison.html
```

Inputs use the existing bounded UTF-8 SRT, WebVTT, ASS and SSA parsers. Exit 0
means compared, including files with changes; 2 means invalid input/comparison
limit, 5 means IO failure, 130 means cancellation. HTML publication is atomic to
a new path. CLI HTML is readable only by default. Add `--embed-package` to include both complete files. `--tolerance-ms`, `--ignore-tags`, `--ignore-line-breaks` and
`--include-unchanged` control comparison/reporting. Whitespace-only differences
are ignored by default. Tolerance defaults to the original format's tick (1 ms
SRT/WebVTT, 10 ms ASS/SSA). Both endpoints must be within tolerance.

## Alignment and output

Cues sort by start, end and source position. Identical normalized text within
tolerance supplies greedy ordered anchors. Within each gap a second pass searches
eight positions either side of the expected index for equal normalized text and
duration. An offset anchor must be unique in both the original and revised
search windows. Two ordered, locally unique matches that agree within tolerance
establish an offset. Cuts, edits and repeated lines may intervene. The next
locally unique match can follow the established offset or propose a replacement.
Repeated lines are aligned with the surrounding timing evidence by the bounded
dynamic program.

Alignment is monotone in time. If a shift carries cues across earlier cues that
remain in place, crossing cues are represented by removals and additions.
Between anchors, dynamic programming
allows matched pairs, additions, removals, splits into two/three and merges of
two/three. Candidate intervals must overlap or be within
`CANDIDATE_WINDOW_MS = 10000`. The callable pure `pair_score` uses interval IoU
(`TIME_WEIGHT = 0.4`) and multiset token Dice similarity on minimal timed text (start, end, and the caller-supplied visible reading text) using the shared reading
projection (`TEXT_WEIGHT = 0.6`). This emphasizes language while distinguishing
simultaneous dialogue. `MIN_PAIR_SCORE = 0.25` excludes weak candidates.
Group steps require `GROUP_TEXT_THRESHOLD = 0.75` and 75% token containment for each part (tokens consumed once). The part intervals must cover at least half of the single cue. Groups earn the same matched-pair credit as one pair, with `GROUP_PENALTY = 0.1` per extra part.
Unmatched cues cost `GAP_COST = 0.65`. Among equally scoring paths at a
prefix boundary, prefer the one with less accumulated variation in the start
offset of successive matches. Fixed traversal order breaks remaining ties.
These constants are implementation policy, not user preferences.

Sparse dynamic programming uses prefix maxima and delayed group publication;
ordinary anchor-free files do not allocate a quadratic matrix. At a transition,
search around both the previous and incoming offsets. Use the greater of the two
offset-adjusted interval overlaps when scoring a candidate. An exact boundary
supplies offset zero. At end of file the last confirmed offset still applies.
The search considers at most 16 candidate starts per original cue, split evenly
between distinct windows. The public pair scorer uses the supplied times directly
and emitted changes always retain their actual endpoints. Cooperative checkpoints
support cancellation.
The budget for two 10,000-cue documents with 5% edits is **2 seconds**, including
snapshots, metrics, alignment, keys and word diffs. Workers must enforce their
own generation/revision/context guards; the engine never owns accepted state.

Kinds are unchanged, text, timing, text-and-timing, added, removed, split, merged
and style. Style/actor changes have a separate flag, including combined changes.
Text/timing/total percentages divide affected original cues by original count;
empty-original percentages are zero. Added cues have their own count. Removed
and split/merged original cues count as text and timing changes. Style-only
changes count in total, separately from text. These are VSF's definitions.

JSON schema version 2 contains `options`, `statistics` and ordered `changes`
(including unchanged groups). Each change has `kind`, `original`, `revised`,
`words` and independent change flags. Sides carry source positions, cue IDs,
integer millisecond endpoints, complete text/reading text, style/actor and the
existing character/CPS/reading-speed metrics. IDs and positions are navigation
hints; snapshots remain meaningful without them. A change `key` is lowercase
SHA-256 over the JSON tuple of kind and both sides' (start, end, text) tuples,
excluding IDs and positions. Identical content produces identical keys.

Word differences use Myers over words, whitespace, punctuation, line breaks and
whole tags. A 512-edit-distance search cap bounds pathological cue allocations;
above it complete removed/added text blocks retain all content. Ignored differences
can remain visible in an included unchanged row. Source text is never interpreted.

## Reports and discussion

HTML has inline CSS, no required script or external requests, escaped names/text,
visible line breaks, metrics and both timings. Change element IDs derive from
keys. The writer accepts explicit trusted extension sections for the head, each
change element ID (including the occurrence suffix for duplicates) and the end, which carry escaped discussions and optional inert review data. See [review packages](review-packages.md).

Unlike Comparo², VSF groups uniform retimes into collapsible blocks. The JSON `blocks` array gives half-open `first..end` indices into the complete ordered changes, signed `delta_ms` and the union timeline span `start_ms..end_ms`, and the revised span `revised_start_ms..revised_end_ms` shown in block labels. At least three consecutive timing-only rows must agree on the offset of both endpoints within tolerance. Other kinds, including unchanged cues, break a block. Statistics, anchors and keys stay per cue.

Compared with Comparo², VSF detects splits/merges, defines its own percentages,
accepts no `.scenechange` input, and supports file-based discussions. Source code
and wording from that service are not reused.

## Desktop review context

Review → Start Review writes a separate `<stem>.reviewed.<ext>` file from saved
content and opens Changes without changing the layout preset. Normal Save then
writes that copy. Save/Discard/Cancel protects unsaved work first. Compare with
Original attaches a file to the ordinary working document; Compare Files asks for
original then revised and works from an empty workspace. If the original has a
newer modification time, Swap/Keep/Cancel makes the direction explicit.

The immutable original is separate from the translation reference; both may be
attached at once. Attach, options and End Review change context rather than
subtitle history. Original and copied-source paths are protected from Save As and
conversion output, including deleted paths. No Mark Reviewed state is stored.
The Changes tab pages at most 200 groups (100 by default).

Comparison captures run on one worker with no queue, a cooperative ten-second
deadline and generation/revision/review-epoch checks. The frontend debounces
accepted changes for 250 ms and polls only its current request. Media is not part
of comparison identity. Closing/opening media does not invalidate comparison.

Project version 4 adds a `review` struct containing independent reference identity,
provenance and comparison options. Older projects retain the minimum version
needed for their contents. Missing/changed originals are resolved before current
work is discarded: choose the expected file or end the review. Recovery version 7
retains original bytes as well, independently of translation; the JSON envelope
is 528 MiB including the version 8 thread budget and metadata. Version 5 projects and version 8 recovery preserve posted discussion and draft/export state. Ordinary subtitle files contain neither relationship.

## Reviewing in context

Changes offers text, timing, additions, removals, split/merge and style filters.
Counts describe the current comparison; there is no stored reviewed state.
F6 / Shift+F6 navigate changes in the active filter. Selecting a row selects its
revised cue(s), frames the time range and shows the original text/timing line.
Removed rows navigate by their original time. Without media, numeric timing
and all document operations remain available; frame tolerance and waveform
originals require media. Opening media does not restart comparison.

Cue markers and the editor diff use bounded current-revision projections. Original
waveform blocks are read-only, visually distinct, and controlled by Show Original
Timing. Revert replaces the affected group with the original text, times and
metadata; Revert Changes applies the active filter as one ordinary Undo step.
Existing IDs are reused where possible, new cues use the document's monotonic ID
allocator, and validation precedes publication. Identical duplicate content keys
use a source-position hint within the guarded capture for precise single revert.
End Review keeps edits and history. Saving subtitles offers project saving once
per review because subtitle files cannot retain the original relationship.

The HTML report includes signed timing, character-count and reading-rate deltas.
Duplicate content keys have deterministic occurrence suffixes in HTML element
IDs. Reports publish to new paths without replacing existing files. Very long
word diffs fall back to complete removed/added blocks, without losing text.

In All and Timing, a collapsed shift block occupies one page row and one navigation stop. Expand it to navigate its cues individually. Selecting the block frames its complete span. Revert Block restores its cues in one Undo step. Text and Timing filters include only matching one-to-one changes (both include text-and-timing); their batch actions restore those entire changes. Markers, editor diffs and ghosts always refer to individual changes.
