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

The start tolerance defaults to zero and allows up to 1000 ms on either side.
An inward start correction to the far edge of a shot window is the exception.
Shot repair defaults to moving inward. Nearest allowed edge also considers the
other direction. Endpoints on the outside edge of a window are permitted.
Scanning is heuristic. Incomplete attached-video coverage blocks application.
With no video the shot family is unavailable and the other families can run.

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
A partial improvement to speed or duration is offered unticked.

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
Rust validates the subset against the captured document, profile and scene data,
then applies one undoable retime. Saving remains a separate action. Recovery and
resync documents keep their Save As protection.

## Differences from the previous tools

The migration captured 99 results from three original planners on 33 synthetic
inputs. The default selections match in 79 comparisons. Seven differences fix
independent findings, seven pair endpoints to preserve another rule, and six
use a smaller trim. Run `cargo test --locked -p vsf-application --test timing_parity`
and the `timing_fix` unit tests to check the frozen results, objective and invariants.

With attached video, an incomplete or unavailable scene scan blocks every timing change. Finish the scan before applying repairs. With no video, only Shot changes is disabled.
A second run can offer further partial improvements after the first accepted pass.
