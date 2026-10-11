//! Bounded file IO and SRT acceptance shared by desktop and CLI.
use crate::srt_limits;
use std::{fs, io::Read, path::Path};
use subtitle_formats::srt;

pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| format!("Cannot read subtitles: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Subtitles must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(srt_limits::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read subtitles: {e}"))?;
    srt_limits::validate_size(bytes.len())?;
    Ok(bytes)
}

pub fn parse(bytes: &[u8]) -> Result<srt::Parsed, String> {
    srt_limits::validate_size(bytes.len())?;
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Subtitles must be UTF-8; convert this file's encoding before opening it")?;
    let parsed = srt::parse(text)?;
    srt_limits::validate_document(&parsed.document, &parsed.source)?;
    Ok(parsed)
}
