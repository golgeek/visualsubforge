# Magnetic timing and portable presets

Use the **Snap** toggle in the transport strip; configure its policy in
Settings › Timing and snapping.
Cue gaps and scene buffers can be enabled independently. Drag a cue edge near a
guide or drag a new waveform range; release commits the result. Hold **Alt/Option**
to bypass. Arrow-key nudges stay exact (10 ms, Shift 100 ms, Alt 1 ms). Escape
cancels the preview and restores an enabled loop. Changing settings never retimes
existing cues or runs a batch quality check.

- Between cues: a start snaps to another cue's end plus the gap; an end snaps to
  another cue's start minus the gap. Hatched bands show these targets while dragging.
- Before cut: a cue end can snap this many milliseconds before a scene marker.
- After cut: a cue start can snap this many milliseconds after a scene marker.
  Violet ruler brackets remain visible at rest; hatched bands appear during a drag. Hide **Scenes** to remove scene targets too.
- Snap distance is measured in logical pixels, so zoom controls timing precision.
  Nearest valid target wins; ties prefer a cue gap, then the earlier timestamp.
  Targets inside another cue/gap, inside an overlapping scene buffer, outside
  media or producing zero duration are skipped. Intentional manual overlaps remain
  possible. An entire cue/range may still span other cues or cuts.

Defaults are optional/off, with 80 ms between cues, 130 ms before/after cuts and
8 px snap distance. These are starting values, not a broadcaster's standard.
Values are milliseconds in this increment. Frame-based policies need actual
frame-time mapping, especially for VFR; average FPS conversion is not provided.
Scene targets come only from visual analysis; raw keyframe candidates are no
longer displayed or used for snapping. Review detected cuts: snapping does not
improve detector accuracy.

## Share a preset

**Export timing preset** writes a `.vsfpreset` JSON file. **Import timing preset**
validates the complete file before replacing current settings. Cancel/error leaves
settings intact. Projects store a snapshot of these settings, so sharing a project
does not require installing its original preset. Older projects default to
snapping off. Without a project, settings last for the current app session.

A website can offer [this example](examples/example-timing.vsfpreset) as a download.
Change the name and values to the intended norm; no plugin package or executable
code is needed. A UTF-8 version-1 document has exactly:

```json
{
  "version": 1,
  "name": "My timing policy",
  "timing": {
    "enabled": true,
    "snapCues": true,
    "snapScenes": true,
    "cueGapMs": 80,
    "sceneBeforeMs": 130,
    "sceneAfterMs": 130,
    "snapDistancePx": 8
  }
}
```

All fields are required. Buffer values are integers from 0 to 10000 milliseconds;
snap distance is an integer from 1 to 32 pixels. Names contain 1–120 printable
characters. Files are limited to 256 KiB. Unknown fields/versions are rejected
rather than silently dropping rules. `.json` imports use the same schema;
exports use `.vsfpreset`. Export labels the current values as `Custom timing`;
publishers can supply a descriptive name in their distributed JSON.

This is a timing-policy format, not yet a full standards/quality profile. Reading
speed, maximum duration, punctuation and plugin parameters are future extensions.
Those will need an explicit schema version/compatibility contract and must not
silently change what version 1 means.

## VSS compatibility

Import also accepts UTF-8 VSS `.ini` files and maps this verified subset:

| VSS section/key | VSF field |
| --- | --- |
| `Misc/EnableMouseSnapping` | `enabled` |
| `Misc/SpaceKeyBlankBetweenSubtitles` | `cueGapMs` |
| `WAVDisplay/SceneChangeStartOffset` | `sceneBeforeMs` |
| `WAVDisplay/SceneChangeStopOffset` | `sceneAfterMs` |

Names are matched without case sensitivity; duplicate mapped keys are rejected.
Missing mapped fields use VSF defaults. Import reports how many fields were mapped
and how many other entries were ignored. Plugin parameters, keyboard shortcuts,
anti-overlap rules, scene filtering and frame conversion are not imported. No
legacy JavaScript is loaded or executed. INI files with no supported timing fields,
negative offsets or invalid values fail explicitly. UTF-16/legacy code pages and
export back to VSS are not supported. This is timing-subset migration, not full
VSS preset compatibility.

## Timeline display

Solid violet lines and ruler markers show visual shot changes. The gap lane uses
neutral marks for sufficient gaps, warning marks below the effective minimum and
error marks for overlaps. Distance tags mark cue endpoints inside a shot buffer;
hatching collapses below six logical pixels. Multiple endpoints near one cut share
a tag showing the nearest distance and the number of affected edges. These are display aids, not a second
quality report. Existing accepted Issues and **Snap to cut…** open the Rust-owned
review; snapping lands on a legal buffer endpoint, not necessarily the cut itself.

The minimap shows full-file density, accepted issue dots, cut ticks and the visible
window. Drag or use arrows/Home/End to navigate. Partial cut coverage is labelled;
bounded reads never silently treat viewport cuts as the whole file. The video
scrubber shows the same window and previews a seek until release. Escape, blur,
resize, media replacement or disabling cancels it without seeking. Cut buttons use
the existing scene navigation. Native video remains above the scrubber, never
under a DOM control. Real cut-detection accuracy and native playback require the
optional media build; browser fixtures model them.
