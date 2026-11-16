# Embedded subtitle extraction

`vsf extract --list video.mkv --json` lists every subtitle stream, including
unavailable picture-based and teletext tracks. `vsf extract video.mkv --track 2
-o video.fr.srt` extracts one text track to a new file. Existing outputs and source
aliases are refused. FFmpeg and ffprobe follow the same packaged/development tool
discovery as other analysis operations.

SRT, ASS/SSA and WebVTT use FFmpeg stream copy; MP4 `mov_text` converts to SRT
and styling may differ. ASS/SSA headers and restored event order are retained.
The preserved ScriptType selects `.ass` or `.ssa`; FFmpeg reports both under
the ASS codec/muxer.
WebVTT header blocks already lost by the container muxer cannot be recovered.
Picture tracks require OCR; OCR, teletext, in-video closed captions and remuxing
are outside this feature.

Extraction omits `-copyts`: FFmpeg rebases packets to container start, matching
waveform and playback project time zero. ASS header comments remain untouched.
Text must be UTF-8 (optional BOM), and extracted output is capped at 16 MiB.
The default deadline is thirty minutes. Cancellation terminates and reaps the
owned child, and incomplete results never become documents or output files.

An equal SRT/WebVTT endpoint is repaired to the next strictly later cue start,
capped at five seconds; the last such cue gets five seconds. Repairs are reported
by one-based cue number. Only actual timing headers are repaired; timestamp-like
cue text and metadata stay unchanged. Reversed ranges remain errors. Overlaps and source order
are retained. Empty tracks produce valid empty documents.

Suggested names use the video stem, language code and `.forced`/`.sdh` flags.
Common ISO 639-2 language codes normalize to their ISO 639-1 forms. Untrusted
language metadata cannot introduce path separators.

Generate only original synthetic fixtures:

```sh
python3 scripts/media-fixtures.py
cargo test --locked -p media-analysis -p vsf-application -p vsf-cli
cargo test --locked -p vsf-application --test subtitle_extraction -- --ignored
```

Native playback, embedded-font appearance and physical timing alignment require
separate desktop observation; headless tests do not establish those properties.
