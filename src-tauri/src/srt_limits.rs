//! Application acceptance policy shared by open, edits, save and recovery.
//! The serializer owns byte accounting; the domain knows nothing about SRT limits.
use subtitle_core::Document;
use subtitle_formats::srt;

pub const MAX_BYTES: usize = 16 * 1024 * 1024;

pub fn validate_size(length: usize) -> Result<(), String> {
    if length > MAX_BYTES {
        Err(
            "Subtitles exceed the 16 MiB limit; shorten the document before accepting this change"
                .into(),
        )
    } else {
        Ok(())
    }
}

pub fn validate_document(document: &Document, source: &srt::Source) -> Result<(), String> {
    validate_size(srt::serialized_len(document, source)?)
}
