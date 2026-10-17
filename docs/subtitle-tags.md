# Subtitle tags

VSF preserves subtitle text, including tags, through SRT open/edit/save and
Undo/Redo. Accepted text is rendered on native video; unapplied editor drafts
remain separate. Formatting is edited as text. Full `.ass`/`.ssa` file import,
style sheets, attachments and metadata editing are not implemented.

## Preview syntax

Balanced SRT `<b>`, `<i>` and `<u>` tags render as bold, italic and underline.
Nested copies keep the outer style active. Mismatched/unclosed HTML style tags
remain literal. Other HTML tags and entities remain literal as before.

Complete ASS/SSA override blocks such as `{\an8\b1}` now pass to mpv/libass.
This supports its inline formatting and layout syntax, including font/size/color,
border/shadow/alpha, ASS `\an` and SSA `\a` alignment, position/movement, fades,
transforms, karaoke, clipping and drawing. The application does not implement a
second tag renderer. Unknown or invalid parameters follow libass behavior; they
can be ignored in preview, but their original text remains editable and saved.
An unclosed override block stays literal instead of hiding its remaining text.

A complete override block opts **that cue** into ASS text escapes: `\N` hard
break, `\n` soft break, `\h` nonbreaking space and libass's escaped braces.
Use `{\r}` to opt in without changing the default style. Backslashes in ordinary
SRT cues (including Windows paths) stay literal even when another cue uses ASS.
Physical newlines remain hard breaks. Mixed HTML/ASS styles apply in text order;
closing an HTML style emits its off override, rather than restoring an earlier
ASS setting.

For example:

```text
{\an8\1c&H0000FF&\b1}Red title at the top
{\r}First line\NSecond line
{\move(30,140,350,140)\fs32\fad(100,100)}Moving text
```

SRT has no script resolution or style sheet. Preview uses a 384×288 script
canvas, a `Default` style at size 20, bottom-center alignment and 10-unit margins.
Position/size overrides use that canvas. The established Mac default font is
Hiragino Sans; other platforms use sans-serif. Explicit font names depend on
installed fonts. Named-style resets cannot recover a style absent from SRT.
This is inline-tag support, not fidelity to an external ASS file's missing
headers/styles/fonts.

## Timing, ownership and counting

The internal ASS preview uses Matroska subtitle packets with 1 ms timestamps and
durations. Exporting an intermediate ASS text file would lose millisecond
precision. An application-owned, bounded serializer builds one subtitle track;
mpv receives it through its public `hex://` memory source and owns the copied
bytes. Plain documents retain the escaped WebVTT memory path. No temporary file,
extra FFmpeg process or native dependency patch is required during editing.
The transport is capped at 4 MiB, including hex expansion for styled documents.
Exceeding that limit disables preview for the revision; editing/saving continue.

The existing media-control worker serializes replacements after releasing the
document lock. It hides/removes the old track, sets the matching ASS style mode,
then selects the new accepted revision. Failure remains visible in the preview
status and cannot make stale text look current. Track cleanup, session guards,
and native timing/composition ownership are unchanged. Preview never mutates
the accepted document or undo history.

Quality counting is a separate named policy. `vss_utf16_v1` removes complete
override blocks, including unknown ones, but counts literal `\N` as two UTF-16
units and does not turn it into a line boundary. Drawing instructions outside
override blocks are counted as text. Its line lengths describe the historical
counted source, not rendered wrapping, animation or drawing. Pixel/typography
checks and full broadcaster compliance remain outside the current checker.

## Validation

Ordinary Rust tests cover literal/malformed syntax, mixed/nested styles,
transport bounds, accepted edit/Undo/Redo/save preservation, mode switching and
replacement errors. To exercise production packets with native tools:

```sh
cargo test --locked -p visualsubforge native_demux_preserves_milliseconds -- --ignored --nocapture
python3 scripts/check-ass-preview.py --mpv /absolute/path/to/mpv --output ./tests/fixtures/generated/quality/ass-composition-new
```

The first command requires FFprobe and proves exact cue boundaries at 1.001 and
2.999 seconds, overlaps, packet text and original SRT preservation. The second requires
FFmpeg and an mpv executable with the image output; it renders production bytes
and checks SSA alignment/mixed italic/breaks, ASS color, advancing motion and
expiry. Its macOS/Linux Unix-socket diagnostic also switches ASS/WebVTT sources
30 times through public `sub-add`/`sub-remove`, verifies only one external track
at a time, clears it and exits normally. It records tool/source hashes and retained frames. These are real native
software-composition checks, not desktop/GPU/audio or Windows/Linux acceptance.

The format follows the [Matroska subtitle mapping](https://www.matroska.org/technical/subtitles.html#ssaass-subtitles)
and the [mpv memory-stream interface](https://mpv.io/manual/master/#protocols).
Inline tag interpretation belongs to libass, as used by mpv.
