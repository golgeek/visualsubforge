# Settings and shortcuts

Settings has a searchable sidebar: General, Editing, Timing and snapping,
Quality profile, Spelling and language, Video and shot changes, Shortcuts,
Appearance, Plugins and Advanced. Counting units and speed calculation live in
Advanced; profile limitations remain under About this profile. Appearance points
to existing layout/column controls; Plugins points to the existing package manager.

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

## Shortcut presets

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
bindings are handled by the webview. Confirmed changes update menus, command
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
