//! Attachment record boundaries and overlays only; font interpretation belongs
//! to the application layer. Original blocks are streamed, authored blocks shared.
use super::{Result, Source};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    Font,
    Graphic,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    /// Stable within this immutable source lineage, guarded by document revision.
    pub id: usize,
    pub kind: AttachmentKind,
    pub name: String,
    pub encoded_bytes: usize,
    pub problem: Option<&'static str>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct AttachmentInventory {
    pub total: usize,
    pub entries: Vec<Attachment>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Overlays {
    replacements: BTreeMap<usize, Replacement>,
    additions: Vec<Option<Arc<FontRecord>>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Replacement {
    end: usize,
    font: Option<Arc<FontRecord>>,
}
#[derive(Debug, PartialEq, Eq)]
struct FontRecord {
    name: String,
    encoded_bytes: usize,
    raw: String,
}
struct Block<'a> {
    start: usize,
    end: usize,
    kind: AttachmentKind,
    name: &'a str,
    problem: Option<&'static str>,
}
const MAX_FONTS: usize = 32;
const MAX_ENCODED_BYTES: usize = 6 * 1024 * 1024;

impl Source {
    /// Limit is capped at 200, even for files containing many opaque attachments.
    /// Inspection never decodes or copies the encoded payload into the inventory.
    pub fn attachments(&self, limit: usize) -> AttachmentInventory {
        let mut result = AttachmentInventory {
            total: 0,
            entries: Vec::new(),
        };
        self.visit_attachments(|block, authored| {
            result.total += 1;
            if result.entries.len() < limit.min(200) {
                let (name, encoded_bytes) = if let Some(font) = authored {
                    (font.name.as_str(), font.encoded_bytes)
                } else {
                    (
                        block.name,
                        self.0.lines[block.start + 1..block.end]
                            .iter()
                            .map(|l| body(&l.raw).len())
                            .sum(),
                    )
                };
                result.entries.push(Attachment {
                    id: block.start,
                    kind: block.kind,
                    name: name.chars().take(128).collect(),
                    encoded_bytes,
                    problem: block.problem.or_else(|| {
                        (name.len() > 128).then_some("Attachment name exceeds 128 UTF-8 bytes")
                    }),
                });
            }
        });
        result
    }
    /// Streams encoded lines without line endings or the name record. Callers
    /// enforce decoded-size limits and cancellation during interpretation.
    pub fn read_attachment(
        &self,
        id: usize,
        mut read: impl FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        let mut result = Err("Attachment no longer exists".into());
        self.visit_attachments(|block, authored| {
            if block.start == id {
                result = if let Some(font) = authored {
                    font.raw.lines().skip(1).try_for_each(&mut read)
                } else {
                    self.0.lines[block.start + 1..block.end]
                        .iter()
                        .try_for_each(|line| read(body(&line.raw)))
                };
            }
        });
        result
    }
    /// Pure record transformation. The application must validate the decoded font
    /// and the final whole-document size before accepting this source property.
    pub fn with_font(&self, id: Option<usize>, name: &str, encoded: &str) -> Result<Self> {
        if name.is_empty()
            || name.len() > 128
            || name.trim() != name
            || name.chars().any(char::is_control)
            || name.contains(['/', '\\', ':', '[', ']'])
            || name == "."
            || name == ".."
        {
            return Err("Font attachment name must be a filename of at most 128 UTF-8 bytes without paths, controls, colons or brackets".into());
        }
        if encoded.is_empty()
            || encoded.len() > MAX_ENCODED_BYTES
            || encoded.lines().any(|line| {
                line.is_empty() || line.len() > 80 || !line.bytes().all(|b| (33..=96).contains(&b))
            })
        {
            return Err("Font encoding must fit 6 MiB in nonempty ASCII 33–96 lines of at most 80 characters".into());
        }
        let mut count = 0;
        let mut duplicate = false;
        let mut target = None;
        self.visit_attachments(|block, authored| {
            if block.kind == AttachmentKind::Font {
                count += 1;
                let old_name = authored.map_or(block.name, |f| f.name.as_str());
                duplicate |= Some(block.start) != id && old_name.eq_ignore_ascii_case(name);
            }
            if Some(block.start) == id {
                target = Some((block.end, block.kind, block.problem));
            }
        });
        if count > MAX_FONTS || (id.is_none() && count == MAX_FONTS) {
            return Err("Font management supports at most 32 active fonts".into());
        }
        if duplicate {
            return Err("A font attachment with this name already exists".into());
        }
        if id.is_some() {
            validate_target(target)?;
        }
        let mut raw = format!("fontname: {name}{}", self.0.newline);
        for line in encoded.lines() {
            raw.push_str(line);
            raw.push_str(self.0.newline);
        }
        let font = Arc::new(FontRecord {
            name: name.into(),
            encoded_bytes: encoded.lines().map(str::len).sum(),
            raw,
        });
        let mut overlays = self.1.fonts.clone();
        match id {
            Some(id) if id < self.0.lines.len() => {
                overlays.replacements.insert(
                    id,
                    Replacement {
                        end: target.expect("validated target").0,
                        font: Some(font),
                    },
                );
            }
            Some(id) => {
                overlays.additions[id - self.0.lines.len()] = Some(font);
            }
            None => {
                overlays.additions.push(Some(font));
            }
        }
        Ok(self.with_font_overlays(overlays))
    }
    pub fn without_font(&self, id: usize) -> Result<Self> {
        let mut target = None;
        self.visit_attachments(|block, _| {
            if block.start == id {
                target = Some((block.end, block.kind, block.problem));
            }
        });
        validate_target(target)?;
        let mut overlays = self.1.fonts.clone();
        if id < self.0.lines.len() {
            overlays.replacements.insert(
                id,
                Replacement {
                    end: target.expect("validated target").0,
                    font: None,
                },
            );
        } else {
            overlays.additions[id - self.0.lines.len()] = None;
        }
        Ok(self.with_font_overlays(overlays))
    }
    fn with_font_overlays(&self, fonts: Overlays) -> Self {
        Self(
            Arc::clone(&self.0),
            Arc::new(super::Authoring {
                fonts,
                ..self.1.as_ref().clone()
            }),
        )
    }
    fn visit_attachments(&self, mut visit: impl FnMut(Block<'_>, Option<&FontRecord>)) {
        self.original_attachments(|block| {
            if let Some(change) = self.1.fonts.replacements.get(&block.start) {
                if let Some(font) = &change.font {
                    visit(block, Some(font));
                }
            } else {
                visit(block, None);
            }
        });
        for (index, font) in self.1.fonts.additions.iter().enumerate() {
            if let Some(font) = font {
                visit(
                    Block {
                        start: self.0.lines.len() + index,
                        end: self.0.lines.len() + index,
                        kind: AttachmentKind::Font,
                        name: &font.name,
                        problem: None,
                    },
                    Some(font),
                );
            }
        }
    }
    fn original_attachments(&self, mut visit: impl FnMut(Block<'_>)) {
        let mut section = None;
        let mut active: Option<Block<'_>> = None;
        for (index, line) in self.0.lines.iter().enumerate() {
            let text = body(&line.raw).trim_start_matches('\u{feff}').trim();
            // Canonical font data can itself begin '[' and end ']'. Only a
            // recognized header, or text outside the encoding alphabet, can
            // delimit it. Guessing a section here would split valid fonts.
            let encoded_line = !text.is_empty()
                && text.len() <= 80
                && text.bytes().all(|b| (33..=96).contains(&b));
            if text.starts_with('[')
                && text.ends_with(']')
                && (section != Some(AttachmentKind::Font)
                    || recognized_header(text)
                    || !encoded_line)
            {
                if let Some(mut block) = active.take() {
                    block.end = index;
                    if block.kind == AttachmentKind::Font && !recognized_header(text) {
                        block.problem = Some(
                            "An unknown section after this font may be parsed as font data by players; preserve it unchanged",
                        );
                    }
                    visit(block);
                }
                section = if text.eq_ignore_ascii_case("[Fonts]") {
                    Some(AttachmentKind::Font)
                } else if text.eq_ignore_ascii_case("[Graphics]") {
                    Some(AttachmentKind::Graphic)
                } else {
                    None
                };
            } else if let Some(kind) = section
                && let Some((key, name)) = text.split_once(':')
                && key.eq_ignore_ascii_case(if kind == AttachmentKind::Font {
                    "fontname"
                } else {
                    "filename"
                })
            {
                if let Some(mut block) = active.take() {
                    block.end = index;
                    visit(block);
                }
                active = Some(Block {
                    start: index,
                    end: 0,
                    kind,
                    name: name.trim(),
                    problem: (kind == AttachmentKind::Font && key != "fontname").then_some(
                        "Font name record uses noncanonical case; players may ignore it",
                    ),
                });
            }
        }
        if let Some(mut block) = active {
            block.end = self.0.lines.len();
            visit(block);
        }
    }
    /// Returns the replacement and its original exclusive end. Empty output
    /// removes only that block; the caller must retain its prior newline state.
    pub(super) fn font_replacement(&self, index: usize) -> Option<(&str, usize)> {
        self.1
            .fonts
            .replacements
            .get(&index)
            .map(|r| (r.font.as_ref().map_or("", |f| f.raw.as_str()), r.end))
    }
    pub(super) fn write_added_fonts(&self, ended: &mut bool, mut write: impl FnMut(&str)) {
        let mut fonts = self.1.fonts.additions.iter().flatten().peekable();
        if fonts.peek().is_none() {
            return;
        }
        if !*ended {
            write(self.0.newline);
        }
        write("[Fonts]");
        write(self.0.newline);
        for font in fonts {
            write(&font.raw);
        }
        *ended = true;
    }
}
fn validate_target(target: Option<(usize, AttachmentKind, Option<&str>)>) -> Result<()> {
    let (_, kind, problem) = target.ok_or("Font attachment no longer exists")?;
    if kind != AttachmentKind::Font {
        return Err("Graphic attachments are preserved read-only".into());
    }
    if let Some(problem) = problem {
        return Err(problem.into());
    }
    Ok(())
}
fn body(raw: &str) -> &str {
    raw.strip_suffix("\r\n")
        .or_else(|| raw.strip_suffix('\n'))
        .unwrap_or(raw)
}
fn recognized_header(text: &str) -> bool {
    [
        "[Script Info]",
        "[V4 Styles]",
        "[V4+ Styles]",
        "[Events]",
        "[Fonts]",
    ]
    .iter()
    .any(|h| text.eq_ignore_ascii_case(h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::{parse, serialize, serialized_len};
    use subtitle_core::Editor;
    const ASS: &str = include_str!("../../../../tests/fixtures/subtitles/styles.ass");
    const SSA: &str = include_str!("../../../../tests/fixtures/subtitles/styles.ssa");
    fn payload(source: &Source, id: usize) -> String {
        let mut value = String::new();
        source
            .read_attachment(id, |line| {
                value.push_str(line);
                Ok(())
            })
            .unwrap();
        value
    }
    #[test]
    fn font_overlays_preserve_unrelated_bytes_and_share_history_storage() {
        for base in [ASS, SSA] {
            let original = format!("\u{feff}{}[Graphics]\nfilename: preserved.bmp\nOPAQUE!\n[Fonts]\nfontname: One.ttf\n!!!!\nfontname: Two.otf\n####", base).replace('\n', "\r\n");
            let parsed = parse(&original).unwrap();
            let inventory = parsed.source.attachments(200);
            assert_eq!(inventory.total, 3);
            assert_eq!(inventory.entries[0].kind, AttachmentKind::Graphic);
            assert_eq!(inventory.entries[1].encoded_bytes, 4);
            let one = inventory.entries[1].id;
            let two = inventory.entries[2].id;
            assert_eq!(payload(&parsed.source, one), "!!!!");
            let replacement = parsed
                .source
                .with_font(Some(one), "Changed.ttf", "$$$$\n%%%%")
                .unwrap();
            assert!(Arc::ptr_eq(&parsed.source.0, &replacement.0));
            assert_eq!(payload(&replacement, one), "$$$$%%%%");
            let expected = original.replace(
                "fontname: One.ttf\r\n!!!!\r\n",
                "fontname: Changed.ttf\r\n$$$$\r\n%%%%\r\n",
            );
            assert_eq!(serialize(&parsed.document, &replacement).unwrap(), expected);
            assert_eq!(
                serialized_len(&parsed.document, &replacement).unwrap(),
                expected.len()
            );
            assert!(
                replacement
                    .preview_header()
                    .contains("fontname: Changed.ttf\r\n$$$$\r\n%%%%")
            );
            let added = replacement.with_font(None, "Third.ttf", "&&").unwrap();
            let appended = format!("{expected}\r\n[Fonts]\r\nfontname: Third.ttf\r\n&&\r\n");
            assert_eq!(serialize(&parsed.document, &added).unwrap(), appended);
            assert_eq!(
                serialized_len(&parsed.document, &added).unwrap(),
                appended.len()
            );
            let id = added.attachments(200).entries[3].id;
            let changed_addition = added.with_font(Some(id), "Third.ttf", "(()").unwrap();
            assert_eq!(payload(&changed_addition, id), "(()");
            let styled = changed_addition
                .with_style(
                    Some(1),
                    changed_addition.styles()[1]
                        .with_field("Fontsize", "42")
                        .unwrap(),
                )
                .unwrap();
            assert!(Arc::ptr_eq(
                added.1.fonts.replacements[&one].font.as_ref().unwrap(),
                styled.1.fonts.replacements[&one].font.as_ref().unwrap()
            ));
            assert!(Arc::ptr_eq(
                changed_addition.1.fonts.additions[0].as_ref().unwrap(),
                styled.1.fonts.additions[0].as_ref().unwrap()
            ));
            let removed = replacement.without_font(two).unwrap();
            assert!(removed.read_attachment(two, |_| Ok(())).is_err());
            assert_eq!(
                serialize(&parsed.document, &removed).unwrap(),
                expected.replace("fontname: Two.otf\r\n####", "")
            );
            let cleared_addition = added.without_font(id).unwrap();
            assert_eq!(
                serialize(&parsed.document, &cleared_addition).unwrap(),
                expected
            );
            let mut editor =
                Editor::with_properties(parsed.document.clone(), parsed.source.clone());
            editor
                .apply_document(0, parsed.document.clone(), styled, |d, s| {
                    serialized_len(d, s).map(|_| ())
                })
                .unwrap();
            let accepted = serialize(editor.document(), editor.properties()).unwrap();
            assert!(accepted.contains("Title,sans-serif,42"));
            editor.undo(1).unwrap();
            assert_eq!(
                serialize(editor.document(), editor.properties()).unwrap(),
                original
            );
            editor.redo(2).unwrap();
            assert_eq!(
                serialize(editor.document(), editor.properties()).unwrap(),
                accepted
            );
            let reopened = parse(&accepted).unwrap();
            assert_eq!(
                serialize(&reopened.document, &reopened.source).unwrap(),
                accepted
            );
        }
    }
    #[test]
    fn inspection_is_bounded_and_opaque_or_ambiguous_records_are_preserved() {
        let original = format!(
            "{ASS}[Fonts]\nfontname: Ambiguous.ttf\n!!!!\n[Unknown]\nOpaque: retain\n[Fonts]\nFontname: WrongCase.ttf\n####\n[Graphics]\nfilename: image.bmp\n????\n"
        );
        let parsed = parse(&original).unwrap();
        let inventory = parsed.source.attachments(1);
        assert_eq!(inventory.total, 3);
        assert_eq!(inventory.entries.len(), 1);
        assert!(
            inventory.entries[0]
                .problem
                .unwrap()
                .contains("unknown section")
        );
        for entry in parsed.source.attachments(200).entries {
            assert!(
                parsed
                    .source
                    .with_font(Some(entry.id), "replacement.ttf", "!!!!")
                    .is_err()
            );
            assert!(parsed.source.without_font(entry.id).is_err());
        }
        assert_eq!(
            serialize(&parsed.document, &parsed.source).unwrap(),
            original
        );
        let mut many = ASS.to_owned();
        many.push_str("[Graphics]\n");
        for index in 0..250 {
            many.push_str(&format!("filename: {index}.bmp\n!!!!\n"));
        }
        let parsed = parse(&many).unwrap();
        let inventory = parsed.source.attachments(usize::MAX);
        assert_eq!((inventory.total, inventory.entries.len()), (250, 200));
        assert_eq!(serialize(&parsed.document, &parsed.source).unwrap(), many);
    }
    #[test]
    fn encoded_bracket_lines_are_data_not_unknown_sections() {
        let original =
            format!("{ASS}[Fonts]\nfontname: Brackets.ttf\n[!!]\n!!!!\nfontname: Next.ttf\n####\n");
        let parsed = parse(&original).unwrap();
        let inventory = parsed.source.attachments(200);
        assert_eq!(inventory.total, 2);
        assert!(inventory.entries[0].problem.is_none());
        assert_eq!(payload(&parsed.source, inventory.entries[0].id), "[!!]!!!!");
        let changed = parsed
            .source
            .with_font(Some(inventory.entries[0].id), "Brackets.ttf", "$$$$")
            .unwrap();
        assert_eq!(
            serialize(&parsed.document, &changed).unwrap(),
            original.replace("[!!]\n!!!!\n", "$$$$\n")
        );
    }
    #[test]
    fn attachment_names_limits_missing_ids_and_stream_failures_are_checked() {
        let parsed = parse(ASS).unwrap();
        for name in [
            "",
            "../bad.ttf",
            "a\\bad.ttf",
            "bad:font.ttf",
            "[Fonts]",
            " bad.ttf",
            "bad\n.ttf",
        ] {
            assert!(
                parsed.source.with_font(None, name, "!!!!").is_err(),
                "{name:?}"
            );
        }
        for encoded in ["", "abcd", "\n!!!!", "!!!!\n\n####", "!!!!\r####"] {
            assert!(
                parsed.source.with_font(None, "font.ttf", encoded).is_err(),
                "{encoded:?}"
            );
        }
        assert!(
            parsed
                .source
                .with_font(None, "font.ttf", &"!".repeat(81))
                .is_err()
        );
        assert!(
            parsed
                .source
                .with_font(Some(0), "font.ttf", "!!!!")
                .is_err()
        );
        assert!(parsed.source.without_font(usize::MAX).is_err());
        let mut source = parsed.source.clone();
        for index in 0..32 {
            source = source
                .with_font(None, &format!("font{index}.ttf"), "!!!!")
                .unwrap();
        }
        assert!(source.with_font(None, "too-many.ttf", "!!!!").is_err());
        assert!(source.with_font(None, "FONT0.TTF", "!!!!").is_err());
        let first = source.attachments(1).entries[0].id;
        assert_eq!(
            source.read_attachment(first, |_| Err("Cancelled".into())),
            Err("Cancelled".into())
        );
        let removed = source.without_font(first).unwrap();
        assert!(
            removed
                .with_font(Some(first), "missing.ttf", "!!!!")
                .is_err()
        );
        assert!(removed.with_font(None, "new.ttf", "!!!!").is_ok());
        assert_eq!(serialize(&parsed.document, &parsed.source).unwrap(), ASS);
    }
}
