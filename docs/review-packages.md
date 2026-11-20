# Offline review packages

A `.vsfreview` file contains the complete original and revised subtitles, including format metadata, the comparison options and the discussion. A VSF comparison report can carry the same data in its HTML head. Share either file directly with the people who should receive both versions. Names are self-chosen display names. Anyone can reproduce an ID or write under another name.

## Command line

```sh
vsf compare original.srt revised.srt --package round1.vsfreview --html comparison.html
vsf review show round1.vsfreview --json
vsf review show comparison.html
vsf review merge ana.vsfreview sam.vsfreview -o combined.vsfreview
```

Outputs publish atomically to new paths. Merge requires identical original fingerprints. The highest round supplies the revised file and options, with latest export time breaking ties. Further ties compare the revised SHA-256, exporter, names, serialized options and formats in that order. The CLI reports the selected input. Merge preserves the maximum round.

## Version 1 wire format

The media type is `application/vnd.visualsubforge.review+json`. The UTF-8 JSON uses snake_case keys at every level. The [exact fixture](../crates/vsf-application/tests/fixtures/review-v1.vsfreview) includes a cue thread. All fields are required except message text on status events. Unknown fields or versions are rejected.

| Object | Fields |
| --- | --- |
| Package | `format` = `visualsubforge-review`, `version` = 1, positive integer `round`, `exported_by`, integer `exported_at_ms`, `original`, `revised`, `options`, `threads` |
| Subtitle | `name` (sanitized filename), `format` (srt, vtt, ass, ssa), `sha256` (lowercase hex of the exact bytes), `bytes` (standard padded base64) |
| Options | Integer `tolerance_ms` and booleans `ignore_tags`, `ignore_line_breaks`, `ignore_whitespace` |
| Thread | `id`, `anchor`, `first_message_id`, `messages` |
| Change anchor | `kind` = change, `change_kind`, `change_key`, arrays `original` and `revised` |
| Cue anchor | `kind` = cue, single-element array `revised` |
| Anchor cue | Integer `start_ms`, `end_ms`, exact subtitle `text` |
| Message | `id`, `author`, Unix millisecond `at_ms`, `type` (comment, resolve, reopen), `text` for comments |

Hash the compact UTF-8 JSON array of [anchor, first author, first timestamp, first text] for the thread ID. Anchor object fields follow the table order. Hash [thread ID, author, timestamp, type, text or null] for each message ID. SHA-256 hex is lowercase. The explicit first message ID identifies the creator even when a peer's clock is behind. Display events in (timestamp, ID) order. The latest resolve/reopen in that order determines status. Merging unions both sets by ID. Once exchanged, messages are immutable.

Change anchors first match the comparison content key. Fallback searches within ten seconds of the snapshot's start using the comparison pair score. A change can match another changed group with the same sides present. Average the available original/revised scores, or compare the revised cue alone for cue anchors. Require at least 0.25. Prefer the closest time, then earliest comparison index. A missing match leaves a detached discussion with its immutable snapshot.

## Limits and report safety

The package/report limit is 64 MiB. Each decoded subtitle may occupy 16 MiB. UTF-8 and BOM-marked UTF-16 embedded data pass through the format parser. Fingerprints always cover the original bytes. A reader checks both fingerprints and all identities before adoption. The discussion limit is 10,000 threads, 100,000 messages, 10,000 Unicode characters per message and 100 per author. Names, anchor text and comment text together may occupy 16 MiB of UTF-8. Only line breaks and tabs are allowed as controls in text. Names allow no controls. CLI exports can have an empty exporter.

The report displays escaped plain text. Exactly one inert script element with the review media type and ID `vsf-review-package` carries the JSON. The writer escapes less-than, greater-than, ampersand, U+2028 and U+2029 inside that block. Import reads only its bounded JSON payload.
