//! Embedded subtitle adoption policy. No desktop state or native playback owner.
use crate::{atomic_file, new_output, subtitle_file};
use media_analysis::{
    Cancellation, Tools,
    subtitle_tracks::{self, Track},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Provenance {
    pub media_path: PathBuf,
    pub stream_index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub suggested_name: String,
}
pub struct Prepared {
    pub parsed: subtitle_formats::Parsed,
    pub provenance: Provenance,
    pub font_count: u32,
    pub repaired_cues: Vec<u32>,
}
pub fn language_code(code: &str) -> &str {
    match code {
        "fre" | "fra" => "fr",
        "eng" => "en",
        "ger" | "deu" => "de",
        "spa" => "es",
        "ita" => "it",
        "por" => "pt",
        "jpn" => "ja",
        "chi" | "zho" => "zh",
        "kor" => "ko",
        "rus" => "ru",
        "dut" | "nld" => "nl",
        "ara" => "ar",
        "pol" => "pl",
        "swe" => "sv",
        "nor" => "no",
        "dan" => "da",
        "fin" => "fi",
        "cze" | "ces" => "cs",
        "gre" | "ell" => "el",
        "tur" => "tr",
        "und" => "",
        _ => code,
    }
}
pub fn suggested_name(path: &Path, track: &Track) -> String {
    let mut name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if name.is_empty() {
        name = "Subtitles".into();
    }
    if let Some(language) = &track.language {
        let lower = language.to_lowercase();
        let code = language_code(&lower);
        // Metadata is a label, never a path. Reject separators/control characters.
        if !code.is_empty()
            && code.len() <= 32
            && code.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            name.push('.');
            name.push_str(code);
        }
    }
    if track.forced {
        name.push_str(".forced");
    }
    if track.hearing_impaired {
        name.push_str(".sdh");
    }
    name.push('.');
    name.push_str(track.extension().unwrap_or("srt"));
    name
}
pub fn prepare(path: &Path, extracted: subtitle_tracks::Extracted) -> Result<Prepared, String> {
    crate::srt_limits::validate_size(extracted.bytes.len())?;
    let track = extracted.track;
    let text = std::str::from_utf8(&extracted.bytes)
        .map_err(|_| format!("Track {} isn't valid UTF-8 text", track.index))?;
    let extension = track.extension().ok_or("Unsupported subtitle codec")?;
    let (text, repaired_cues) = repair_missing_duration(text, extension)?;
    // FFmpeg uses its ASS muxer for both script variants. Codec metadata cannot
    // distinguish SSA v4 from ASS v4+: the preserved script header owns that fact.
    let parsed =
        subtitle_file::parse_document(text.as_bytes(), (extension != "ass").then_some(extension))?;
    if extension == "ass" && parsed.source.ass().is_none() {
        return Err("ASS/SSA track did not contain a supported script header".into());
    }
    let mut name = PathBuf::from(suggested_name(path, &track));
    name.set_extension(parsed.source.format().extension());
    Ok(Prepared {
        parsed,
        provenance: Provenance {
            media_path: path.to_owned(),
            stream_index: track.index,
            codec: track.codec.clone(),
            language: track.language.clone(),
            title: track.title.clone(),
            suggested_name: name.to_string_lossy().into_owned(),
        },
        font_count: extracted.font_count,
        repaired_cues,
    })
}
pub fn extract(
    tools: &Tools,
    path: &Path,
    index: u32,
    cancel: &Cancellation,
    timeout: Duration,
) -> Result<Prepared, String> {
    let result =
        subtitle_tracks::extract(tools, path, index, cancel, timeout).map_err(|e| e.to_string())?;
    if cancel.is_cancelled() {
        return Err("Analysis cancelled".into());
    }
    let result = prepare(path, result)?;
    if cancel.is_cancelled() {
        return Err("Analysis cancelled".into());
    }
    Ok(result)
}
pub fn publish(prepared: &Prepared, output: &Path, cancel: &Cancellation) -> Result<(), String> {
    let output = new_output::resolve(output).map_err(|e| e.to_string())?;
    new_output::protect_source(&output, &prepared.provenance.media_path)
        .map_err(|e| e.to_string())?;
    prepared
        .parsed
        .source
        .validate_save_extension(output.extension().and_then(|e| e.to_str()))?;
    let text = prepared
        .parsed
        .source
        .serialize(&prepared.parsed.document)?;
    let pending = atomic_file::prepare_with_checkpoint(&output, text.as_bytes(), || {
        if cancel.is_cancelled() {
            Err("Analysis cancelled".into())
        } else {
            Ok(())
        }
    })?;
    if cancel.is_cancelled() {
        return Err("Analysis cancelled".into());
    }
    pending.commit_new()
}

// FFmpeg emits explicit equal endpoints for absent durations. Repair only these,
// never negative/reversed ranges. Fixed five-second cap avoids hidden dependence
// on desktop preferences; use the next strictly later start when nearer.
fn repair_missing_duration(text: &str, extension: &str) -> Result<(String, Vec<u32>), String> {
    let pattern = regex::Regex::new(if extension == "ass" {
        r"(?m)^Dialogue: *[^,]*,(\d{1,4}:\d{2}:\d{2}\.\d{2}),(\d{1,4}:\d{2}:\d{2}\.\d{2}),"
    } else if extension == "vtt" {
        r"(?m)^((?:\d{2,}:)?\d{2}:\d{2}\.\d{3}) --> ((?:\d{2,}:)?\d{2}:\d{2}\.\d{3})"
    } else {
        r"(?m)^(\d{2,4}:\d{2}:\d{2}[,.]\d{3}) --> (\d{2,4}:\d{2}:\d{2}[,.]\d{3})"
    })
    .map_err(|e| e.to_string())?;
    // A timestamp-looking line in cue text is content, not another cue. Collect
    // only actual block headers before applying the equal-endpoint policy.
    let mut headers = std::collections::HashSet::new();
    let (mut offset, mut block_line, mut timing_seen, mut metadata, mut events) =
        (0, 0, false, false, false);
    for raw in text.split_inclusive('\n') {
        let line = raw
            .trim_end_matches(['\r', '\n'])
            .trim_start_matches('\u{feff}');
        if line.trim().is_empty() {
            block_line = 0;
            timing_seen = false;
            metadata = false;
        } else {
            if line.starts_with('[') {
                events = line.eq_ignore_ascii_case("[Events]");
            }
            if block_line == 0 {
                metadata = line.starts_with("WEBVTT")
                    || line == "STYLE"
                    || line == "REGION"
                    || line == "NOTE"
                    || line.starts_with("NOTE ")
                    || line.starts_with("NOTE\t");
            }
            let header = match extension {
                "ass" => events,
                "vtt" => block_line <= 1 && !timing_seen && !metadata,
                _ => block_line == 1,
            };
            if header && pattern.is_match(line) {
                headers.insert(offset);
                timing_seen = true;
            }
            block_line += 1;
        }
        offset += raw.len();
    }
    let mut times = Vec::new();
    for capture in pattern.captures_iter(text) {
        if !headers.contains(&capture.get(0).unwrap().start()) {
            continue;
        }
        let parse = |s: &str| {
            if extension == "ass" {
                subtitle_formats::ass::parse_time(s).map(|t| t.get())
            } else if extension == "vtt" {
                subtitle_formats::webvtt::parse_time(s).map(|t| t.get())
            } else {
                subtitle_formats::srt::parse_time(&s.replace('.', ","))
                    .map(|t| t.get())
                    .map_err(|e| e.to_string())
            }
        };
        times.push((
            capture.get(2).unwrap().range(),
            parse(&capture[1])?,
            parse(&capture[2])?,
        ));
    }
    let mut starts: Vec<_> = times.iter().map(|(_, start, _)| *start).collect();
    starts.sort_unstable();
    let mut output = text.to_owned();
    let mut repairs = Vec::new();
    for (index, (range, start, end)) in times.iter().enumerate().rev() {
        if start != end {
            continue;
        }
        let next = starts
            .get(starts.partition_point(|s| s <= start))
            .copied()
            .unwrap_or(start.saturating_add(5000));
        let end = subtitle_core::Milliseconds::new(next.min(start.saturating_add(5000)))
            .map_err(|e| e.to_string())?;
        let replacement = if extension == "ass" {
            subtitle_formats::ass::format_time(end)?
        } else if extension == "vtt" {
            subtitle_formats::webvtt::format_time(end)
        } else {
            subtitle_formats::srt::format_time(end)
        };
        output.replace_range(range.clone(), &replacement);
        repairs.push(index as u32 + 1);
    }
    repairs.reverse();
    Ok((output, repairs))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> Track {
        Track {
            index: 3,
            codec: "subrip".into(),
            language: Some("fra".into()),
            title: None,
            default: true,
            forced: true,
            hearing_impaired: true,
            unavailable: None,
        }
    }
    #[test]
    fn duration_repair_preserves_timestamp_looking_text_and_metadata() {
        for (extension, text) in [
            (
                "srt",
                "1\n00:00:01,000 --> 00:00:02,000\n00:00:03,000 --> 00:00:03,000\n",
            ),
            (
                "vtt",
                "WEBVTT\n\nNOTE example\n00:00:03.000 --> 00:00:03.000\n\nid\n00:00:01.000 --> 00:00:02.000\n00:00:03.000 --> 00:00:03.000\n",
            ),
        ] {
            let (result, repaired) = repair_missing_duration(text, extension).unwrap();
            assert_eq!(result, text);
            assert!(repaired.is_empty());
        }
    }
    #[test]
    fn ass_codec_uses_preserved_ssa_header_for_format_and_filename() {
        let mut track = track();
        track.codec = "ass".into();
        let value = prepare(
            Path::new("video.mkv"),
            subtitle_tracks::Extracted {
                bytes: include_bytes!("../../../tests/fixtures/subtitles/styles.ssa").to_vec(),
                track,
                font_count: 0,
            },
        )
        .unwrap();
        assert_eq!(value.parsed.source.format(), subtitle_formats::Format::Ssa);
        assert_eq!(value.provenance.suggested_name, "video.fr.forced.sdh.ssa");
    }
    #[test]
    fn oversized_payload_is_rejected_before_parsing() {
        let extracted = subtitle_tracks::Extracted {
            bytes: vec![b' '; crate::srt_limits::MAX_BYTES + 1],
            track: track(),
            font_count: 0,
        };
        assert!(
            prepare(Path::new("video.mkv"), extracted)
                .err()
                .unwrap()
                .contains("16 MiB")
        );
    }
    #[test]
    fn ass_repairs_only_packet_dialogue_not_header_comments() {
        let text = "[Events]\nComment: 0,0:00:01.00,0:00:01.00,Default,,0,0,0,,Keep\nDialogue: 0,0:00:01.00,0:00:01.00,Default,,0,0,0,,A\nDialogue: 0,0:00:03.00,0:00:04.00,Default,,0,0,0,,B\n";
        let (fixed, repairs) = repair_missing_duration(text, "ass").unwrap();
        assert_eq!(repairs, vec![1]);
        assert!(fixed.contains("Comment: 0,0:00:01.00,0:00:01.00"));
        assert!(fixed.contains("Dialogue: 0,0:00:01.00,0:00:03.00"));
    }
    #[test]
    fn naming_normalizes_language_and_treats_metadata_as_a_label() {
        assert_eq!(
            suggested_name(Path::new("Episode 01.mkv"), &track()),
            "Episode 01.fr.forced.sdh.srt"
        );
        let mut t = track();
        t.language = Some("../../bad".into());
        assert_eq!(
            suggested_name(Path::new("Episode.mkv"), &t),
            "Episode.forced.sdh.srt"
        );
    }
    #[test]
    fn invalid_utf8_is_track_specific_and_empty_is_valid() {
        let input = |bytes| subtitle_tracks::Extracted {
            bytes,
            track: track(),
            font_count: 0,
        };
        assert!(
            prepare(Path::new("video.mkv"), input(vec![255]))
                .err()
                .unwrap()
                .contains("Track 3 isn't valid UTF-8")
        );
        assert!(
            prepare(Path::new("video.mkv"), input(vec![]))
                .unwrap()
                .parsed
                .document
                .cues()
                .is_empty()
        );
    }
    #[test]
    fn absent_duration_ends_at_next_start_with_five_second_cap() {
        let text = "1\n00:00:01,000 --> 00:00:01,000\nA\n\n2\n00:00:03,000 --> 00:00:03,000\nB\n";
        let (fixed, repairs) = repair_missing_duration(text, "srt").unwrap();
        let parsed = subtitle_file::parse_document(fixed.as_bytes(), Some("srt")).unwrap();
        assert_eq!(repairs, vec![1, 2]);
        assert_eq!(parsed.document.cues()[0].end().get(), 3000);
        assert_eq!(parsed.document.cues()[1].end().get(), 8000);
    }
}
