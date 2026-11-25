# Settings and shortcuts

Settings has a searchable sidebar: Editing, Timing and snapping, Quality profile,
Spelling and language, Video and scene changes, Shortcuts, Appearance and Advanced.
Counting units and speed calculation live in Advanced. Built-in profiles are
read-only; Quality, Language and Advanced offer **Customize a copy**. Custom
profiles remain editable after reopening Settings and retain their identity.
Plugins are available from Review → Subtitle Plugins.

Editing includes **Zoom waveform to the selected cue**, enabled by default.
Selecting a cue centers its waveform range and, when enabled, gives it about 60%
of a view clamped to 8–120 seconds and the media duration. Zoom keeps the explicit
Follow Playhead preference; panning suspends it until playback starts or Jump is
chosen. While following is off or suspended, wheel zoom stays anchored at the
pointer in the current view. Temporary suspension is never saved as following off.

View → Zoom In, Zoom Out and Fit Waveform use Cmd/Ctrl+=, Cmd/Ctrl+− and
Cmd/Ctrl+0. Cmd/Ctrl+Shift+= (the + key) also zooms in. These control the
waveform, not WebView scale. Older bundled presets receive unassigned zoom
bindings when conflict-free; custom bindings are preserved.

Valid changes save after a short pause without closing Settings. The footer shows
confirmation and **Undo settings change**, which restores the previous confirmed
preferences without changing subtitle Undo history. Close finishes a valid pending
change. Invalid values remain in their fields, including across page navigation,
with a reason; **Close without saving draft** discards only unaccepted values.
Imports use this same validation path. Export writes the current draft explicitly.

A failed or missing reply does not trigger automatic retries. **Check saved
settings** reads Rust's current policy and saved preferences, keeping the draft.
If the first write succeeded, it is recognized without a duplicate write.
Otherwise review the draft and choose **Retry save**. A newer policy epoch cannot
be overwritten by a stale dialog. Quality-rule counts represent distinct cues in
the accepted report, including related cues, and are unavailable while checking
or stale. They do not count findings or unsaved cue text.

The transport bar starts with Play/Pause and Loop range. Loop range plays the
selected interval. Press it again to clear an active loop. Previous Frame and
Next Frame are available in the Playback menu with their shortcuts.

## Shortcut presets

**Cue → Merge as Dialog** acts on two adjacent selected cues. It starts with no
shortcut. Assign one under Settings → Shortcuts.

Existing settings migrate to **Current VSF bindings**. Replace uses
Cmd/Ctrl+Shift+F, resolving its former macOS collision with Merge with Previous.

**VSF default (trial)** is opt-in. With the list or waveform focused, N creates,
S splits, M/Shift+M merges next/previous, Backspace deletes, Space plays, and
brackets set start/end at the playhead. In cue text, ordinary keys type, Tab plays
and Shift+Tab replays from two seconds earlier. Escape returns to the waveform.
Other fields keep Tab navigation; other buttons keep Enter/Space activation.
Modified variants remain available. Edge edits commit through Rust and reject
nonpositive intervals; source-format precision controls rounding.

**VisualSubSync classic** maps supported actions: F1 play/pause, F2 loop range,
Cmd/Ctrl+O open project, Cmd/Ctrl+D shift and Cmd/Ctrl+Y redo. Other VSF bindings
remain. This is not complete legacy emulation; F4 remains issue navigation.

Search an action and type a chord or use **Record**. Empty removes a binding.
Everywhere and its alternate overlap all focus modes; list/waveform and cue-text
bindings may share a key. Conflicts prevent saving. Escape, text formatting and
OS-reserved keys retain their owners. Ctrl+Alt character bindings are rejected to
preserve AltGr typing; composition is never dispatched as a command.

Native menus display the primary Everywhere binding. Scoped and alternate
bindings are handled by the webview. A primary shortcut that also navigates text
appears in parentheses in its menu item and runs through the focused view.
On macOS, Option+Left/Right seeks from the cue list or waveform. In text fields it
moves by word, and adding Shift selects by word. Command+arrows and their Shift
variants also stay with text fields. On Windows and Linux, text fields keep
Ctrl+arrows and Home/End, including Shift selections. In cue text, Option+Up/Down
still edits the previous or next cue. The Classic preset follows the same focus
rule. Confirmed changes update menus, command
help and tooltips. Rust validates persistence and native publication together,
restoring previous accelerators on failure. A failed restoration is reported
explicitly and requires restarting the app.

Before adopting the trial preset, exercise ordinary typing/IME/AltGr, Tab and
Shift+Tab in cue text versus other controls, N/S/M/Backspace in list and waveform,
Escape focus return, bracket edge edits and Undo. Test a rebind, a conflict,
restart persistence and both menu and keyboard invocation on your platform.

## Appearance

Settings → Appearance selects System, Dark or Light and comfortable or compact
control spacing. Changes use the same confirmed-save and Undo flow as other
settings. System follows the operating system without rewriting the preference
on each color-scheme change. Dark and compact remain the migration defaults.
Control density does not change the row density saved for each layout preset;
choose that separately in Columns. The page body remains transparent for native
video in both themes.


On Linux, **Custom title bar (after restart)** opts into in-app minimize,
maximize/restore and Close buttons. Native decorations remain the default; restart
after enabling or disabling the option. Close still protects unsaved work. macOS
uses inset native traffic lights and a draggable filename area; Windows keeps
native captions and snap behavior. Windows/Linux also provide grouped menus in
the top bar. The filename area supports drag and double-click maximize/restore
when an overlay or custom bar is active.

## Menus and panels

Review → Fix All Timing… opens the Timing tab. Fix gaps and overlaps…, Fix reading speed… and Snap to shot changes… open its single-family presets.

Cue and Review group their editing and checking operations. Review → Issues
(Cmd/Ctrl+1), Statistics (Cmd/Ctrl+2), and View → Translation Reference
(Cmd/Ctrl+3) open docks in the current layout. Closing Translation restores the
Review tab it temporarily replaced. Issue overflow, the title-bar issue count,
and F4 also reveal Issues without selecting a different preset. The toolbar and
Recent work count individual findings as issues; affected-cue counts are labelled
separately. The selected-cue strip groups speed findings into chips but exposes
all metric explanations, including line length, through All explanations.

View → Layout contains all five presets, Arrange Panels and Reset Pane Sizes.
Unresized presets inherit the current preview height; explicitly resized presets
retain their saved height. Presets have no number shortcuts because 1–3 belong
to the review/translation panels. Existing custom bindings remain unchanged;
select a bundled preset in Settings to adopt its current defaults.

Edit → Command Palette and Help → Keyboard Shortcuts replace the title-bar
launchers. macOS registers Help for system menu search. Windows/Linux menus use
the same registry and checked-state projection. View's Theme and Density choices
use the same confirmed preference save path as Settings. Unsupported operations,
including WebVTT conversion, are not offered as working menu actions.

## Version updates

On first launch you can enable automatic checks, decline them, or choose
**Ask next launch**. Change the choice under **Settings → Advanced → Version
updates**. Automatic checks run a few seconds after startup, at most once in 24
hours. **Help → Check for Updates…** and the Command Palette work with either
setting. GitHub receives your IP address and the VSF version. VSF sends no file,
project, machine identifier or credentials. A newer release offers **View
release**, which opens the default browser. **Not now** dismisses that version
until a manual check or a newer version.

For packaged-build diagnostics, `VSF_UPDATE_DIAGNOSTICS=1` prints a single
stderr line when a live request starts. `VSF_UPDATE_TEST_FIXTURE` selects a
local JSON release list and `VSF_UPDATE_TEST_VERSION` overrides the comparison
version for that process. Neither setting accepts a server address. The ordinary
request always targets the public GitHub release list.
