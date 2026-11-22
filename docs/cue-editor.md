# Cue editing

Text, style and exact time fields commit after a 700 ms typing pause, ordinary
blur, moving to another cue or a cue command. Split/Merge/Delete commit pending
fields first. IME composition holds commits until completion. Saving the subtitle
file remains a separate action; “Unsaved changes” means accepted edits have not
been saved to disk.

Escape discards unsent fields, waits for any already dispatched commit, restores
the latest confirmed accepted cue and returns focus to the waveform. It does not
undo an accepted edit. A rejected edit stays visible with an outline and one-line
reason. Retry never blindly replays an unknown outcome: it first refreshes Rust's
state, then asks you to review before another mutation. If the target was removed
or replaced, retained text remains available to copy without attaching to a new cue.

Consecutive text-only commits to the same cue in one focus session coalesce into
one Rust Undo step, with a five-second maximum idle interval between commits.
Timing/style changes, other cues, commands, Undo/Redo and file-save checkpoints
break the group. Formatting first commits pending fields, then records the
formatted text separately. Explicit history buttons use document history;
ordinary native/text-input Undo shortcuts retain their existing text-focus policy.
The interaction defaults are implemented for owner trial and remain adjustable.

A modal or Save/Discard decision retains its existing draft-protection controls.
Its focus transfer does not implicitly commit underneath the decision. Closing
it restarts the pause interval; explicit Apply/Discard controls inside a protected
review keep their original purpose. Ctrl/Cmd+Enter can still commit immediately.

Start/end fields accept exact timecodes. Minus/plus nudge one *nominal* frame at
the container-declared frame rate, rounded to the format's precision (1 ms for
SRT/WebVTT, 10 ms for ASS/SSA). Nudges are disabled when cadence is unavailable.
This is not a variable-frame-rate boundary guarantee; review timing against video.

The selected cue's issue strip stays the same height for one or many issues.
Errors come first, CPS/reading-speed findings share a chip, and +N opens the Issues dock in the current layout. Meters and per-source-line character
counts come from Rust; wrapped display lines do not change source-line counts.
Enable Cue metrics to show the meters and source-line counts. Draft and accepted
metrics are labelled separately.

“Fix N” applies an immutable Rust plan as one Undo step. It combines available
reflow, deterministic language replacements and timing repairs for this cue,
without splitting or changing neighboring cues. Fix N runs the same timing engine as the dock with all three families ticked,
Current cue scope and the saved priority, start tolerance and shot-edge policy.
A text correction that changes counted length can be validated together with
its timing adjustment. The planner checks neighbours when validating the result.
A spacing trade-off follows the selected priority. With no video, shot checks
are unavailable. An incomplete attached-video scan blocks timing changes.
Ambiguous markup and spelling choices remain manual. Preparation considers up
to 200 deterministic language replacements. [Timing policy](timing-repair-all.md)

Preparation shares the existing timing-review worker, has no pending queue and a
ten-second deadline. Changing document/profile/media/scenes/selection or entering
another modal retires the plan. Native video, variable-rate timing and physical
platform input require separate desktop validation; browser fixtures model IPC.

Waveform cue edges within six pixels can start a timing drag even when the cue
is not selected, after more than three pixels of pointer movement. A plain click
keeps cue selection and seek behavior. Pending fields commit before selecting that cue; the captured
pointer is checked again before starting. Edges use the existing snap, preview,
Escape-cancel and single-Undo timing operation. A drag outside the edge tolerance
continues to select a range.
