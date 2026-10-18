//! Conservative single-font sfnt structure checks, not a font sanitizer. Native
//! rendering still owns complete table interpretation and resource lifetime.
use super::MAX_FONT_BYTES;
use serde::Serialize;
use subtitle_core::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum FontKind {
    TrueType,
    OpenTypeCff,
    OpenTypeCff2,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub kind: FontKind,
    pub tables: u16,
    pub glyphs: u16,
    pub family: Option<String>,
}
struct Table<'a> {
    tag: &'a [u8],
    offset: usize,
    length: usize,
    bytes: &'a [u8],
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    let value = bytes.get(at..at + 2).ok_or("Font table is truncated")?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    let value = bytes.get(at..at + 4).ok_or("Font table is truncated")?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}
fn checksum(bytes: &[u8], head: bool, checkpoint: &mut impl FnMut() -> Result<()>) -> Result<u32> {
    let mut sum = 0u32;
    for (index, part) in bytes.chunks(4).enumerate() {
        if index % 16384 == 0 {
            checkpoint()?;
        }
        if head && index == 2 {
            continue;
        }
        let mut word = [0; 4];
        word[..part.len()].copy_from_slice(part);
        sum = sum.wrapping_add(u32::from_be_bytes(word));
    }
    Ok(sum)
}
pub fn inspect(bytes: &[u8], mut checkpoint: impl FnMut() -> Result<()>) -> Result<FontInfo> {
    checkpoint()?;
    if bytes.len() < 12 || bytes.len() > MAX_FONT_BYTES {
        return Err("A single font must fit 4 MiB and contain a complete sfnt header".into());
    }
    let truetype = match bytes.get(..4) {
        Some([0, 1, 0, 0]) => true,
        Some(b"OTTO") => false,
        Some(b"ttcf") => {
            return Err(
                "Font collections are preserved read-only; choose a single TTF/OTF font".into(),
            );
        }
        _ => return Err("Only single sfnt TrueType/OpenType fonts are supported".into()),
    };
    let count = usize::from(u16_at(bytes, 4)?);
    if !(1..=128).contains(&count) {
        return Err("Font table count must be between 1 and 128".into());
    }
    let directory_end = 12 + count * 16;
    if directory_end > bytes.len() {
        return Err("Font table directory is truncated".into());
    }
    let power = 1usize << count.ilog2();
    if usize::from(u16_at(bytes, 6)?) != power * 16
        || u32::from(u16_at(bytes, 8)?) != count.ilog2()
        || usize::from(u16_at(bytes, 10)?) != count * 16 - power * 16
    {
        return Err("Font table search parameters are inconsistent".into());
    }
    let mut tables = Vec::with_capacity(count);
    let mut previous: Option<&[u8]> = None;
    for entry in bytes[12..directory_end].as_chunks::<16>().0 {
        checkpoint()?;
        let tag = &entry[..4];
        if !tag.iter().all(|b| (32..=126).contains(b)) || previous.is_some_and(|p| p >= tag) {
            return Err("Font table tags must be unique and ordered printable ASCII".into());
        }
        previous = Some(tag);
        let offset =
            usize::try_from(u32_at(entry, 8)?).map_err(|_| "Font table offset is too large")?;
        let length =
            usize::try_from(u32_at(entry, 12)?).map_err(|_| "Font table size is too large")?;
        let end = offset
            .checked_add(length)
            .ok_or("Font table bounds overflow")?;
        if offset < directory_end || !offset.is_multiple_of(4) || end > bytes.len() {
            return Err("Font table points outside its aligned data region".into());
        }
        let data = &bytes[offset..end];
        if checksum(data, tag == b"head", &mut checkpoint)? != u32_at(entry, 4)? {
            return Err("Font table checksum does not match its data".into());
        }
        tables.push(Table {
            tag,
            offset,
            length,
            bytes: data,
        });
    }
    let mut ranges: Vec<_> = tables
        .iter()
        .filter(|t| t.length > 0)
        .map(|t| (t.offset, t.offset + t.length))
        .collect();
    ranges.sort_unstable();
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err("Font table data overlaps".into());
    }
    let end = ranges.last().ok_or("Font contains no table data")?.1;
    if bytes.len() > end.next_multiple_of(4) || bytes[end..].iter().any(|b| *b != 0) {
        return Err("Font has unsupported trailing data".into());
    }
    if checksum(bytes, false, &mut checkpoint)? != 0xb1b0_afba {
        return Err("Whole-font checksum is invalid".into());
    }
    let table = |tag: &[u8]| {
        tables
            .iter()
            .find(|t| t.tag == tag)
            .map(|t| t.bytes)
            .ok_or_else(|| {
                format!(
                    "Font is missing required {} table",
                    String::from_utf8_lossy(tag)
                )
            })
    };
    for tag in [
        b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post",
    ] {
        table(tag)?;
    }
    let head = table(b"head")?;
    if head.len() < 54
        || u32_at(head, 0)? != 0x0001_0000
        || u32_at(head, 12)? != 0x5f0f_3cf5
        || !(16..=16384).contains(&u16_at(head, 18)?)
    {
        return Err("Font head table is unsupported or malformed".into());
    }
    let maxp = table(b"maxp")?;
    let glyphs = u16_at(maxp, 4)?;
    if glyphs == 0 {
        return Err("Font contains no glyphs".into());
    }
    let hhea = table(b"hhea")?;
    if hhea.len() < 36 {
        return Err("Font hhea table is truncated".into());
    }
    let metrics = u16_at(hhea, 34)?;
    if metrics == 0
        || metrics > glyphs
        || table(b"hmtx")?.len() < usize::from(metrics) * 4 + usize::from(glyphs - metrics) * 2
    {
        return Err("Font horizontal metrics are inconsistent".into());
    }
    let kind = if truetype {
        if maxp.len() < 32 || u32_at(maxp, 0)? != 0x0001_0000 {
            return Err("TrueType maxp table is unsupported".into());
        }
        let loca = table(b"loca")?;
        let glyf = table(b"glyf")?;
        let long = match u16_at(head, 50)? {
            0 => false,
            1 => true,
            _ => return Err("TrueType glyph location format is unsupported".into()),
        };
        let width = if long { 4 } else { 2 };
        if loca.len() < (usize::from(glyphs) + 1) * width {
            return Err("TrueType glyph locations are truncated".into());
        }
        let mut previous = 0usize;
        for i in 0..=usize::from(glyphs) {
            if i % 512 == 0 {
                checkpoint()?;
            }
            let offset = if long {
                u32_at(loca, i * 4)? as usize
            } else {
                usize::from(u16_at(loca, i * 2)?) * 2
            };
            if offset < previous || offset > glyf.len() {
                return Err("TrueType glyph location is outside its data".into());
            }
            previous = offset;
        }
        FontKind::TrueType
    } else {
        let cff = tables.iter().find(|t| t.tag == b"CFF ");
        let cff2 = tables.iter().find(|t| t.tag == b"CFF2");
        match (cff, cff2) {
            (Some(t), None)
                if t.bytes.len() >= 4
                    && t.bytes[0] == 1
                    && usize::from(t.bytes[2]) <= t.bytes.len()
                    && t.bytes[2] >= 4 =>
            {
                FontKind::OpenTypeCff
            }
            (None, Some(t))
                if t.bytes.len() >= 5
                    && t.bytes[0] == 2
                    && usize::from(t.bytes[2]) <= t.bytes.len()
                    && t.bytes[2] >= 5 =>
            {
                FontKind::OpenTypeCff2
            }
            _ => {
                return Err(
                    "OpenType CFF outline header is missing, ambiguous or unsupported".into(),
                );
            }
        }
    };
    let family = family_name(table(b"name")?)?;
    checkpoint()?;
    Ok(FontInfo {
        kind,
        tables: count as u16,
        glyphs,
        family,
    })
}
fn family_name(bytes: &[u8]) -> Result<Option<String>> {
    let format = u16_at(bytes, 0)?;
    let count = usize::from(u16_at(bytes, 2)?);
    let strings = usize::from(u16_at(bytes, 4)?);
    if format > 1
        || count > 4096
        || 6 + count * 12 > bytes.len()
        || strings < 6 + count * 12
        || strings > bytes.len()
    {
        return Err("Font name table is unsupported or truncated".into());
    }
    if format == 1 {
        let at = 6 + count * 12;
        let languages = usize::from(u16_at(bytes, at)?);
        if languages > 1024 || at + 2 + languages * 4 > strings {
            return Err("Font language names are truncated".into());
        }
        for i in 0..languages {
            name_data(bytes, strings, at + 2 + i * 4)?;
        }
    }
    let mut result: Option<(u8, String)> = None;
    for record in bytes[6..6 + count * 12].as_chunks::<12>().0 {
        let platform = u16_at(record, 0)?;
        let encoding = u16_at(record, 2)?;
        let language = u16_at(record, 4)?;
        let name_id = u16_at(record, 6)?;
        let value = name_data_record(
            bytes,
            strings,
            usize::from(u16_at(record, 8)?),
            usize::from(u16_at(record, 10)?),
        )?;
        if [1, 16].contains(&name_id)
            && (platform == 0 || (platform == 3 && [1, 10].contains(&encoding)))
            && value.len() <= 1024
            && value.len().is_multiple_of(2)
        {
            let decoded = char::decode_utf16(
                value
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| u16::from_be_bytes([p[0], p[1]])),
            )
            .collect::<std::result::Result<String, _>>();
            if let Ok(decoded) = decoded
                && !decoded.is_empty()
                && !decoded.chars().any(char::is_control)
            {
                let priority =
                    u8::from(name_id == 16) * 2 + u8::from(language == 0x0409 || platform == 0);
                if result.as_ref().is_none_or(|(p, _)| *p < priority) {
                    result = Some((priority, decoded));
                }
            }
        }
    }
    Ok(result.map(|(_, name)| name))
}
fn name_data(bytes: &[u8], strings: usize, at: usize) -> Result<&[u8]> {
    name_data_record(
        bytes,
        strings,
        usize::from(u16_at(bytes, at)?),
        usize::from(u16_at(bytes, at + 2)?),
    )
}
fn name_data_record(bytes: &[u8], strings: usize, length: usize, offset: usize) -> Result<&[u8]> {
    bytes
        .get(strings + offset..strings + offset + length)
        .ok_or_else(|| "Font name string points outside its table".into())
}
