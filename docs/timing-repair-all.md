# Fix all timing

Choose **Fix All Timing…** in Commands or **Fix timing issues…** in Issues.
Review → Timing → Fix opens with gaps and overlaps, duration and reading speed,
and shot changes ticked. The three individual repair commands open the same
review with just their family ticked. Scope can be the whole document, selected
rows, filtered rows, or the current cue. Other cues and unticked rules stay
protected. Review → Timing → Shift provides the separate shift and align tools.

Open the policy summary to choose which rules win. **Keep reading speed** protects
CPS and RS. **Keep spacing** allows a speed trade-off only to repair a gap,
overlap or shot change, with speed and the relevant spacing family both enabled.
Each trade-off shows the old and new metric and its limit. **Untick trade-offs**
removes these units from the accepted set. Policy persists between sessions.
Family ticks belong to the current review.

A row that needs another cue’s change names the cue. Ticking the row selects
its prerequisites. Unticking a prerequisite also turns off its dependents.
A partial can replace a later change that used the original timing. Ticking
either alternative turns the other off. The affected row names the partial,
and **Restore default selection** returns to the original review.

The start tolerance defaults to zero and allows up to 1000 ms on either side.
An inward start correction to the far edge of a shot window is the exception.
Shot repair defaults to moving inward. Nearest allowed edge also considers the
other direction. Endpoints on the outside edge of a window are permitted.
Scanning is heuristic. With attached video, an incomplete or unavailable scan
blocks every timing change. With no video, only Shot changes is disabled.

## Choosing endpoints

Rust processes targets in chronological order. A unit can include the following
cue’s delayed start. Related edits share a checkbox. A candidate must preserve
order, positive durations, format precision, minimum and maximum duration
violations, and avoid creating or lengthening an overlap. Speed repair reserves
the configured gap. Unchanged neighbours participate in whole-document checking.
Ambiguous overlap groups, timed markup, event effects and cues above 64 KiB need
manual review.

Duration takes first priority. Keep reading speed then compares speed, gaps and
shot findings in that order. Keep spacing compares gaps, shots and speed.
Ties prefer less start movement, less end movement, shorter duration, then
earlier time. Endpoints use integer milliseconds, with 10 ms ticks for ASS/SSA.
A partial improvement to speed or duration is offered unticked. A second
run can offer further partial improvements after the first accepted pass.

For counted length N and effective duration D in milliseconds, inclusive maximum
CPS C needs D ≥ ceil(1000N/C). An exclusive maximum RS R needs
D ≥ 501 + floor(1000N/R). Minimum CPS bounds D above by floor(1000N/C).
Minimum RS bounds D above by 500 + floor(1000N/R). The profile supplies counting,
reading start and active limits. When speed remains high, manual work gives the
maximum counted length and the amount to shorten. It uses the largest integer
passing every enabled maximum at the indicated timing. RS requires D > 500.
Overlap-aware estimates are identified in the review. Text editing is separate.

## Reviewing and applying

The summary counts findings in scope. Expand a change for its full timecodes or
choose its cue number to navigate. **Edit** opens a manual cue through protected
navigation. Changes and manual work page independently. The waveform shows
selected changes for up to 200 visible cues. Apply includes all selected pages.
After a checkbox edit, checked rows show the original and selected timings.
The endpoint delta includes only the endpoints that row contributes.
An unchecked row previews the result of ticking it, including its prerequisites
and any replaced alternatives. Following a cue keeps its review tab. A pending
cue draft offers Apply, Discard and Cancel in the dock. Under Whole file,
navigation preserves the checked changes, expanded rows and anchored row position.
Switching tabs and returning to Timing restores those choices and the proposal
page. The dock anchors by cue identity and uses the nearest surviving row if a
refresh removes the anchor. Changing the document revision, media session,
scope IDs, policy, families or review context starts a fresh selection. Closing
the dock also ends the selection.
Rust validates the subset and each claimed fix against the captured document, profile and scene data,
then applies one undoable retime. Saving remains a separate action. Recovery and
resync documents keep their Save As protection.

The cue editor’s **Fix N** uses the same default timing selection for Current cue and all families, with the saved policy. It can combine the timing result with a separate safe text correction in one Undo step.

## Differences from the previous tools

The migration captured 99 results from three original planners on 33 synthetic
inputs. The default selections match in 79 comparisons. Seven differences fix
independent findings, seven pair endpoints to preserve another rule, and six
use a smaller trim. Run `cargo test --locked -p vsf-application --test timing_parity`
and the `timing_fix` unit tests to check the frozen results, objective and invariants.

## Headless review

Preview prints the default selection, manual work, scene coverage and a review ID.
The input can be SRT, WebVTT, ASS or SSA. Use a built-in profile ID or a version-2
preset path. Cue numbers are one-based file positions. The following preview
and apply commands use identical planning inputs.

```sh
vsf fix timing --subtitles episode.srt --profile vsf-default --scenes cuts.json --json
vsf fix timing --subtitles episode.srt --profile vsf-default --scenes cuts.json \
  --accept REVIEW_ID --output episode-fixed.srt
```

Use `--fix gaps,speed,shots` (the default), a subset such as `--fix gaps`, or
`--cue 3,4` to narrow the scope. Policy switches are `--priority reading-speed|spacing`,
`--start-tolerance-ms 0..1000` and `--scene-edges inward|nearest`.

A scene file contains mapped integer milliseconds. It must be a JSON object
with exactly these two fields. The file limit is 16 MiB. The reader accepts at most 100,000 timestamps
within the subtitle time range, sorts them and removes duplicates.

```json
{"version":1,"cutsMs":[0,1450,3008]}
```

With nonzero scene buffers, provide a file or pass `--without-scenes` to review
the other requested families. Using `--fix shots` alone with `--without-scenes`
exits 2 before reading any input. With this opt-out, preview exits 0 if the other families
have no remaining findings or 1 if any remain. JSON reports `noMedia` and the
human preview shows “Scene coverage: no video”. A completed empty cuts file explicitly
supplies zero cuts. With zero buffers the coverage is `notRequired`.

Schema 1 includes the input hash, profile fingerprint, file hashes for explicit
presets and cuts, selected families, policy, sorted scope, coverage, before/after
counts, change units, manual rows with budgets and the result hash. Each change
has a one-based display label and zero-based unit index. Sorted `dependsOn`
indices identify prerequisites. Symmetric `conflictsWith` indices identify
mutually exclusive partial alternatives. `endpoints` contains the numerical
edits, `effects` the rate measurements, and `effectsText` the ordered repair
and rate explanations with their warning flags. Each manual row's `detail`
contains a `lead`, up to two `facts`, an optional one-based `blockingCue`,
its rules, and the numeric `budget` and formatted `budgetText` when available.

Rust computes
the SHA-256 review ID over the compact serialized report in schema field order
with `reviewId` empty. Decimal metrics use the Rust JSON serializer. The schema-1 `withheldAll` flag identifies a preview whose safety
check withheld the entire candidate after three evaluations. The human preview
prints the reason above its rows. Preview writes nothing. Apply recomputes the report, matches the ID, verifies the source
files again and atomically publishes a new same-format file. Existing paths,
including symlinks, and aliases of inputs are refused. With zero selected changes
the output preserves the original bytes.

| Exit | Meaning |
| --- | --- |
| 0 | Complete preview with no remaining requested findings, or output written |
| 1 | Requested findings remain in the preview |
| 2 | Invalid input, option, output path or limit |
| 3 | Incomplete scene coverage when checking shots |
| 5 | File or output error |
| 6 | Input changed during publication or the review ID mismatched |
| 130 | Cancelled |

The CLI integration tests run the actual binary. A desktop owner test compares
Rust's paged changes, manual rows and scope counts against the headless report
for identical subtitle bytes, profile, cuts and options.
