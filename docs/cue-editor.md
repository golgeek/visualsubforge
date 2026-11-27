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

On the waveform, a hand marks a cue that a click will select. Its timing edges show a horizontal resize cursor. A crosshair marks a seek or range selection, including a click inside the selected cue when its exact range is already active.

Following a cue link from Issues, Timing or Statistics commits the current draft
before selecting and revealing the target, just like selecting a cue in the list.
The dock stays on the same tab. If the commit is rejected, the editor stays on
the original cue with the draft, its outline and the rejection reason.

Consecutive text-only commits to the same cue in one focus session coalesce into
one Rust Undo step, with a five-second maximum idle interval between commits.
Timing/style changes, other cues, commands, Undo/Redo and file-save checkpoints
break the group. Formatting first commits pending fields, then records the
formatted text separately. Explicit history buttons use document history;
ordinary native/text-input Undo shortcuts retain their existing text-focus policy.
The interaction defaults are implemented for owner trial and remain adjustable.

A tooltip or subtitle action menu hides native video when it overlaps the picture. Opening a cue menu dismisses any tooltip and blocks new ones until the menu and its command finish. Choosing a menu command preserves that visibility until the edit finishes. Dragging a panel splitter hides the picture until release. Dialogs hide it wherever they open.

A modal or Save/Discard decision retains its existing draft-protection controls.
Its focus transfer does not implicitly commit underneath the decision. Closing
it restarts the pause interval. Ctrl/Cmd+Enter can still commit immediately.

Start/end fields accept exact timecodes. Minus/plus nudge one *nominal* frame at
the container-declared frame rate, rounded to the format's precision (1 ms for
SRT/WebVTT, 10 ms for ASS/SSA). Nudges are disabled when cadence is unavailable.
This is not a variable-frame-rate boundary guarantee; review timing against video.

The selected cue's issue strip stays the same height for one or many issues.
Errors come first and CPS/reading-speed findings share a chip.
Start near shot change and End near shot change identify the affected endpoint.
Both +N and All explanations… open the selected cue’s accepted findings. Close returns
focus to the button that opened the dialog. Show all issues in file opens the Issues dock. Meters and per-source-line character
counts come from Rust; wrapped display lines do not change source-line counts.
Cue metrics show the meters and source-line counts by default. Turn off **Cue metrics** to hide them. Draft and accepted
metrics are labelled separately. During a refresh the meters and source-line
gutter keep their size. The last measurements for the same cue show their
source and dim in place if the read takes longer. Issue chips and both
explanation controls remain available while typing. With the issues filter
on, a selected cue whose last issue resolves stays in place with a check mark
until you select another cue.

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

An open timing review keeps its proposal when background cue fixes are requested. The cue strip explains the wait. Cue fixes resume automatically when the timing review is accepted, cancelled or invalidated. Other preview failures offer **Retry fixes**.

Preparation shares the existing timing-review worker, has no pending queue and a
ten-second deadline. Changing document/profile/media/scenes/selection or entering
another modal retires the plan. Native video, variable-rate timing and physical
platform input require separate desktop validation; browser fixtures model IPC.

Waveform cue edges within six pixels can start a timing drag even when the cue
is not selected, after more than three pixels of pointer movement. A plain click
keeps cue selection and seek behavior. Pending fields commit before selecting that cue; the captured
pointer is checked again before starting. Edges use the existing snap, preview,
Escape-cancel and single-Undo timing operation. A drag outside the edge tolerance
continues to select a range. When you drag or arrow-nudge a selected boundary
past the visible window, its handle stays at the edge until you finish the edit.

## Cue list context menu

Right-click a selected cue to act on the selection. Right-clicking a different
cue commits any pending draft and selects that row. The context-menu key and
Shift+F10 open the focused row’s menu. Escape returns focus to that row.

One cue offers Edit cue text, Split in half, Merge with previous, Merge with next and Delete.
**Split in Half** in the Cue menu performs the same edit. Both halves keep the full
text. Rust rounds the midpoint to the nearest millisecond for SRT/WebVTT or
centisecond for ASS/SSA. An exact tie gives the first half the extra tick. Both
menus explain when the cue is too short or its format prevents splitting.
One Undo restores the original cue. **Split at Playhead** remains available for
choosing a different boundary.
For a group, Merge needs consecutive cues in file order, including cues hidden by
filters. Delete accepts a discontinuous selection. Each operation takes one Undo
step. A merge keeps the first cue’s ID, the earliest start and the latest end.
The cues must have compatible metadata and pass the source format’s merge checks.
The group menu shows the format’s reason beside each unavailable action.

**Merge as Dialog** joins two adjacent cues into two speaker lines. Each cue needs visible text after formatting. It flattens
line breaks within each side and keeps existing hyphens and en/em dashes.
ASS/SSA retain leading overrides before each speaker prefix and use `\N{\r}`
between the two sides.
