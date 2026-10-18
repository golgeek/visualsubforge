//! Bounded font payload interpretation; no native font engine or filesystem IO.
mod sfnt;
pub use sfnt::{FontInfo, FontKind, inspect};
use subtitle_core::Result;
use subtitle_formats::ass;
pub const MAX_FONT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TOTAL_FONT_BYTES: usize = 8 * 1024 * 1024;

pub fn decode(
    source: &ass::Source,
    id: usize,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut pending = 0u32;
    let mut bits = 0u32;
    let mut count = 0usize;
    source.read_attachment(id, |line| {
        checkpoint()?;
        if line.len() > 80 || !line.bytes().all(|b| (33..=96).contains(&b)) {
            return Err("Font attachment has unsupported encoding; expected ASCII 33–96 in lines of at most 80 characters".into());
        }
        for digit in line.bytes() {
            count += 1;
            pending = (pending << 6) | u32::from(digit - 33);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                if bytes.len() == MAX_FONT_BYTES { return Err("Font attachment exceeds the 4 MiB decoded limit".into()); }
                bytes.push((pending >> bits) as u8);
                pending &= (1 << bits) - 1;
            }
        }
        Ok(())
    })?;
    if bytes.is_empty() || count % 4 == 1 || pending != 0 {
        return Err("Font attachment has truncated or noncanonical encoding".into());
    }
    checkpoint()?;
    Ok(bytes)
}

pub fn encode(bytes: &[u8], mut checkpoint: impl FnMut() -> Result<()>) -> Result<String> {
    if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
        return Err("Font data must contain 1 byte through 4 MiB".into());
    }
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4 + bytes.len() / 60);
    let mut column = 0;
    for (index, part) in bytes.chunks(3).enumerate() {
        if index % 1024 == 0 {
            checkpoint()?;
        }
        let value = (u32::from(part[0]) << 16)
            | (u32::from(*part.get(1).unwrap_or(&0)) << 8)
            | u32::from(*part.get(2).unwrap_or(&0));
        for shift in [18, 12, 6, 0].into_iter().take(part.len() + 1) {
            if column == 80 {
                output.push('\n');
                column = 0;
            }
            output.push(char::from(33 + ((value >> shift) & 63) as u8));
            column += 1;
        }
    }
    checkpoint()?;
    Ok(output)
}
