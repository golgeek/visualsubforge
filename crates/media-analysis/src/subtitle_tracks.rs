//! Bounded embedded-text extraction. Blocking; callers supply worker ownership.
//! FFmpeg rebases packet timestamps to container start (project time zero).
//! Do not add -copyts: ASS header comments are not packets and must not shift.
use crate::{Cancellation, Error, Result, Tools, process};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

pub const MAX_SUBTITLE_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Track {
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub unavailable: Option<String>,
}
impl Track {
    pub fn extension(&self) -> Option<&'static str> {
        match self.codec.as_str() {
            "subrip" | "mov_text" => Some("srt"),
            "ass" | "ssa" => Some("ass"),
            "webvtt" => Some("vtt"),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Listing {
    pub tracks: Vec<Track>,
    pub font_count: u32,
    pub fonts: Vec<FontAttachment>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FontAttachment {
    pub index: u32,
    pub filename: Option<String>,
    pub mimetype: Option<String>,
}
#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    streams: Vec<Stream>,
}
#[derive(Deserialize)]
struct Stream {
    index: u32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    #[serde(default)]
    tags: Tags,
    #[serde(default)]
    disposition: Disposition,
}
#[derive(Default, Deserialize)]
struct Tags {
    language: Option<String>,
    title: Option<String>,
    mimetype: Option<String>,
    filename: Option<String>,
}
#[derive(Default, Deserialize)]
struct Disposition {
    #[serde(default)]
    default: u32,
    #[serde(default)]
    forced: u32,
    #[serde(default)]
    hearing_impaired: u32,
}
fn parse_listing(bytes: &[u8]) -> Result<Listing> {
    let probe: Probe = serde_json::from_slice(bytes)
        .map_err(|e| Error::Message(format!("Invalid subtitle stream listing: {e}")))?;
    let mut listing = Listing {
        tracks: Vec::new(),
        font_count: 0,
        fonts: Vec::new(),
    };
    for stream in probe.streams {
        if stream.codec_type.as_deref() == Some("attachment")
            && stream.tags.mimetype.as_deref().is_some_and(|m| {
                matches!(
                    m,
                    "application/x-truetype-font"
                        | "application/vnd.ms-opentype"
                        | "font/ttf"
                        | "font/otf"
                        | "application/x-font-ttf"
                        | "application/x-font-opentype"
                )
            })
        {
            listing.font_count += 1;
            listing.fonts.push(FontAttachment {
                index: stream.index,
                filename: stream.tags.filename.clone(),
                mimetype: stream.tags.mimetype.clone(),
            });
        }
        if stream.codec_type.as_deref() != Some("subtitle") {
            continue;
        }
        let codec = stream.codec_name.unwrap_or_else(|| "unknown".into());
        let unavailable = match codec.as_str() {
            "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" => None,
            "hdmv_pgs_subtitle" | "dvd_subtitle" | "dvb_subtitle" | "xsub" => {
                Some("Picture-based: needs OCR".into())
            }
            "dvb_teletext" => Some("Teletext isn't supported".into()),
            _ => Some(format!("Unsupported subtitle codec: {codec}")),
        };
        listing.tracks.push(Track {
            index: stream.index,
            codec,
            language: stream.tags.language,
            title: stream.tags.title,
            default: stream.disposition.default != 0,
            forced: stream.disposition.forced != 0,
            hearing_impaired: stream.disposition.hearing_impaired != 0,
            unavailable,
        });
    }
    listing.tracks.sort_by_key(|track| track.index);
    Ok(listing)
}
pub fn list(
    tools: &Tools,
    path: &Path,
    cancel: &Cancellation,
    timeout: Duration,
) -> Result<Listing> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(Error::Message("Video must be a regular file".into()));
    }
    let mut command = Command::new(&tools.ffprobe);
    command.args(["-v", "error", "-show_entries", "stream=index,codec_type,codec_name:stream_tags=language,title,filename,mimetype:stream_disposition=default,forced,hearing_impaired", "-of", "json"]).arg(path);
    parse_listing(&process::run(command, cancel, timeout, |r| {
        process::limited(r, 1024 * 1024)
    })?)
}
pub struct Extracted {
    pub bytes: Vec<u8>,
    pub track: Track,
    pub font_count: u32,
}
pub fn extract(
    tools: &Tools,
    path: &Path,
    index: u32,
    cancel: &Cancellation,
    timeout: Duration,
) -> Result<Extracted> {
    let started = Instant::now();
    let path = path.canonicalize()?;
    let listing = list(tools, &path, cancel, timeout)?;
    let track = listing
        .tracks
        .into_iter()
        .find(|track| track.index == index)
        .ok_or_else(|| Error::Message(format!("Subtitle track {index} was not found")))?;
    if let Some(reason) = &track.unavailable {
        return Err(Error::Message(reason.clone()));
    }
    let format = match track.extension() {
        Some("vtt") => "webvtt",
        Some(format) => format,
        None => return Err(Error::Message("Unsupported subtitle track".into())),
    };
    let mut command = Command::new(&tools.ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-map",
            &format!("0:{index}"),
            "-vn",
            "-an",
            "-dn",
            "-c:s",
            if track.codec == "mov_text" {
                "srt"
            } else {
                "copy"
            },
            "-f",
            format,
            "pipe:1",
        ]);
    let bytes = process::run(
        command,
        cancel,
        timeout
            .checked_sub(started.elapsed())
            .ok_or(Error::Timeout)?,
        |r| process::limited(r, MAX_SUBTITLE_BYTES),
    )?;
    Ok(Extracted {
        bytes,
        track,
        font_count: listing.font_count,
    })
}

/// Dump attachment bytes into the owned bounded pipe, never a metadata filename.
/// Attachments are header data; zero video frames avoid scanning the movie.
pub fn extract_font(
    tools: &Tools,
    path: &Path,
    index: u32,
    cancel: &Cancellation,
    timeout: Duration,
) -> Result<Vec<u8>> {
    let start = Instant::now();
    let path = path.canonicalize()?;
    if !list(tools, &path, cancel, timeout)?
        .fonts
        .iter()
        .any(|font| font.index == index)
    {
        return Err(Error::Message(
            "Selected stream is not a supported font attachment".into(),
        ));
    }
    let mut command = Command::new(&tools.ffmpeg);
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            &format!("-dump_attachment:{index}"),
            "pipe:1",
            "-i",
        ])
        .arg(path)
        .args(["-map", "0:v:0?", "-frames:v", "0", "-f", "null", "-"]);
    process::run(
        command,
        cancel,
        timeout.checked_sub(start.elapsed()).ok_or(Error::Timeout)?,
        |reader| process::limited(reader, 4 * 1024 * 1024),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_ffprobe_picture_declaration_is_visible_but_unavailable() {
        let listing = parse_listing(include_bytes!(
            "../../../tests/fixtures/subtitles/picture-track-listing.json"
        ))
        .unwrap();
        let picture = listing
            .tracks
            .iter()
            .find(|track| track.index == 2)
            .unwrap();
        assert_eq!(picture.codec, "dvd_subtitle");
        assert_eq!(
            picture.unavailable.as_deref(),
            Some("Picture-based: needs OCR")
        );
        assert!(picture.extension().is_none());
    }
    #[test]
    fn classifies_all_streams_without_hiding_unavailable_tracks() {
        let listing = parse_listing(br#"{"streams":[{"index":0,"codec_type":"video"},{"index":3,"codec_type":"subtitle","codec_name":"hdmv_pgs_subtitle"},{"index":1,"codec_type":"subtitle","codec_name":"ass","tags":{"language":"fra","title":"Signs"},"disposition":{"default":1,"forced":1}},{"index":4,"codec_type":"subtitle","codec_name":"dvb_teletext"},{"index":5,"codec_type":"subtitle","codec_name":"unknown"}]}"#).unwrap();
        assert_eq!(
            listing.tracks.iter().map(|t| t.index).collect::<Vec<_>>(),
            vec![1, 3, 4, 5]
        );
        assert!(listing.tracks[0].default && listing.tracks[0].forced);
        assert_eq!(listing.tracks[0].extension(), Some("ass"));
        assert_eq!(
            listing.tracks[1].unavailable.as_deref(),
            Some("Picture-based: needs OCR")
        );
        assert_eq!(
            listing.tracks[2].unavailable.as_deref(),
            Some("Teletext isn't supported")
        );
        assert!(listing.tracks[3].unavailable.is_some());
    }
}
