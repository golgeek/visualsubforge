# Offline review packages

A `.vsfreview` file contains the complete original and revised subtitles, including format metadata, the comparison options and the discussion. A VSF comparison report can carry the same data in its HTML head. Share either file directly with the people who should receive both versions. Names are self-chosen display names. Anyone can reproduce an ID or write under another name.

## Command line

```sh
vsf compare original.srt revised.srt --package round1.vsfreview --html comparison.html --embed-package
vsf review show round1.vsfreview --json
vsf review show comparison.html
vsf review merge ana.vsfreview sam.vsfreview -o combined.vsfreview
```

CLI HTML is readable only by default. `--embed-package` opts into including both subtitle files, independently of `--package`. Outputs publish atomically to new paths. Merge requires identical original fingerprints. The highest round supplies the revised file and options, with latest export time breaking ties. Further ties compare the revised SHA-256, exporter, names, serialized options and formats in that order. The CLI reports the selected input. Merge preserves the maximum round.

## Version 1 wire format

The media type is `application/vnd.visualsubforge.review+json`. Readers accept a leading UTF-8 BOM in JSON and HTML. Rust timestamps locally accepted discussion events. The UTF-8 JSON uses snake_case keys at every level. The [exact fixture](../crates/vsf-application/tests/fixtures/review-v1.vsfreview) includes a cue thread. Message text is omitted on status events, and the first occurrence of a change omits its occurrence field. Unknown fields or versions are rejected.

| Object | Fields |
| --- | --- |
| Package | `format` = `visualsubforge-review`, `version` = 1, positive integer `round`, `exported_by`, integer `exported_at_ms`, `original`, `revised`, `options`, `threads` |
| Subtitle | `name` (sanitized filename), `format` (srt, vtt, ass, ssa), `sha256` (lowercase hex of the exact bytes), `bytes` (standard padded base64) |
| Options | Integer `tolerance_ms` and booleans `ignore_tags`, `ignore_line_breaks`, `ignore_whitespace` |
| Thread | `id`, `anchor`, `first_message_id`, `messages` |
| Change anchor | `kind` = change, `change_kind`, `change_key`, optional zero-based `occurrence` for duplicates, arrays `original` and `revised` |
| Cue anchor | `kind` = cue, single-element array `revised` |
| Anchor cue | Integer `start_ms`, `end_ms`, exact subtitle `text` |
| Message | `id`, `author`, Unix millisecond `at_ms`, `type` (comment, resolve, reopen), `text` for comments |

Hash the compact UTF-8 JSON array of [anchor, first author, first timestamp, first text] for the thread ID. Anchor object fields follow the table order. Hash [thread ID, author, timestamp, type, text or null] for each message ID. SHA-256 hex is lowercase. The explicit first message ID identifies the creator even when a peer's clock is behind. Display events in (timestamp, ID) order. The latest resolve/reopen in that order determines status. Merging unions both sets by ID. Once exchanged, messages are immutable.

Change anchors first match the comparison content key and its occurrence in comparison order. The first occurrence omits the field, preserving earlier IDs. If fewer matches survive, use the last available occurrence. Fallback searches within ten seconds of the snapshot's start using the comparison pair score. A change can match another changed group with the same sides present and positive time overlap between at least one corresponding pair of original or revised cues. Average the available original/revised scores, or compare the revised cue alone for cue anchors. Require at least 0.25. Prefer the closest time, then earliest comparison index. A missing match leaves a detached discussion with its immutable snapshot.

## Limits and report safety

The package/report limit is 64 MiB. Each decoded subtitle may occupy 16 MiB. UTF-8 and BOM-marked UTF-16 embedded data pass through the format parser. Fingerprints always cover the original bytes. A reader checks both fingerprints and all identities before adoption. The discussion limit is 10,000 threads, 100,000 messages, 10,000 Unicode characters per message and 100 per author. Names, anchor text and comment text together may occupy 16 MiB of UTF-8. Comment text allows line breaks and tabs. Anchor snapshots follow subtitle-core cue rules and preserve the source text. Names allow no controls. CLI exports can have an empty exporter.

The report displays escaped plain text. Exactly one inert script element with the review media type and ID `vsf-review-package` carries the JSON. The writer escapes less-than, greater-than, ampersand, U+2028 and U+2029 inside that block. Import reads only its bounded JSON payload.

## Desktop workflow

Start Review or attach an original. In Changes or the cue editor, choose Comment. A cue can have a discussion even when it matches the original. On first use enter the display name that recipients will see. Change it later in Settings → Editing. Reply, Resolve and Reopen append events. Delete draft is available until a message has been exported. Delete replies first if removing a draft that started a thread. Imported messages have already been exchanged. Comments affect the project and recovery checkpoint only. The unposted composer stays local to the current view.

Open threads and Commented show individual changes, including cues inside collapsed shift blocks. Counts on blocks include their constituent cues. Previous/Next Open Thread visit individual discussions. After a revert, unmatched discussions appear under Detached with their original snapshot. Go to Time frames that snapshot on the timeline.

Review → Export Review Package includes the accepted working document, even when unsaved. If your name preference is empty, enter a name for this export. Each successful package or embedded HTML export records the display name and time and advances the round. HTML reports include both subtitle versions by default. Choose Readable only to omit the embedded package. Readable-only exports require no exporter name and preserve the round and draft status. The visible discussion still discloses names and messages.

Open Review Package accepts either file. A different original opens an untitled document with both versions embedded. Save As proposes a sanitized revised filename beside the package. With the same original and revised fingerprints, opening merges only the messages. If working content differs, Keep Mine merges discussion, Use Their Version also replaces the cues in one Undo step, and Cancel keeps the current review. Undo restores cue content and leaves the discussion in place. The original always stays the comparison base.

Save a project to retain threads, export history, round and the link to a package-sourced original. Reopening a moved package offers relocation or End Review. Recovery includes accepted drafts and the original contents and can restore without the package.
