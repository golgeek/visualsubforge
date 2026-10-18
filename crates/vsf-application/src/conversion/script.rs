use super::{ConversionFormat, ConversionReport, ConversionSeverity};
use crate::authoring::styles::default_style;
use subtitle_core::{Cue, CueMetadata, Document, Result};
use subtitle_formats::{
    Source,
    ass::{AttachmentKind, Style, Variant},
};

pub(super) fn header(
    document: &Document,
    source: &Source,
    target: ConversionFormat,
    report: &mut ConversionReport,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<String> {
    let script = source.ass();
    if target == ConversionFormat::Srt {
        let script = script.expect("cross-format SRT source is ASS/SSA");
        for style in script.styles() {
            checkpoint()?;
            report.issue(
                "styles_removed",
                ConversionSeverity::Loss,
                "SRT cannot store style definitions.",
                1,
                None,
                &style.name,
            );
        }
        for (name, value) in script.script_info() {
            checkpoint()?;
            report.issue("script_info_removed", ConversionSeverity::Loss, "SRT cannot store script settings or metadata, including canvas, wrapping and color interpretation.", 1, None, &format!("{name}: {value}"));
        }
        let inventory = script.attachments(8);
        report.issue("attachments_removed", ConversionSeverity::Loss, "SRT cannot embed fonts or graphics. Required fonts must be available to the destination player.", inventory.total as u32, None,
            &inventory.entries.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "));
        omitted_records(document, source, report, checkpoint)?;
        return Ok(String::new());
    }
    let variant = if target == ConversionFormat::Ass {
        Variant::Ass
    } else {
        Variant::Ssa
    };
    let mut output = format!(
        "[Script Info]\nScriptType: {}\n",
        if variant == Variant::Ass {
            "v4.00+"
        } else {
            "v4.00"
        }
    );
    if let Some(script) = script {
        for (name, value) in script.script_info() {
            checkpoint()?;
            if !name.eq_ignore_ascii_case("ScriptType") {
                output.push_str(&format!("{name}: {value}\n"));
            }
        }
        report.issue("script_settings", ConversionSeverity::Notice, "Script Info fields are retained; unspecified defaults and unsupported settings can behave differently in the other script version.", 1, None, "The converter does not infer missing canvas or renderer-specific settings.");
        omitted_records(document, source, report, checkpoint)?;
    } else {
        output.push_str("PlayResX: 384\nPlayResY: 288\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: None\n");
        report.issue("default_style", ConversionSeverity::Notice, "SRT has no style sheet. The export creates Default (sans-serif, 24 pt), a 384×288 canvas and explicit wrapping/color defaults.", 1, None, "Inline overrides take precedence; installed font availability and player defaults affect appearance.");
    }
    output.push_str(if variant == Variant::Ass {
        "\n[V4+ Styles]\n"
    } else {
        "\n[V4 Styles]\n"
    });
    let defaults = default_style(variant);
    let styles = script.map_or(&[][..], |s| s.styles());
    if styles.len() > 1024 {
        report.issue(
            "style_limit",
            ConversionSeverity::Blocked,
            "Conversion supports at most 1024 styles.",
            1,
            None,
            "Reduce the style inventory before converting.",
        );
    } else if styles.is_empty() {
        if script.is_some() {
            report.issue("missing_styles", ConversionSeverity::Loss, "The source has no style definitions; an explicit Default replaces renderer-dependent fallback.", 1, None, "Default uses sans-serif at 24 pt.");
        }
        write_style(&mut output, &defaults);
    } else {
        for style in styles {
            checkpoint()?;
            if style.fields.len() > 64 || style.fields.iter().any(|(_, v)| v.len() > 1024) {
                report.issue(
                    "style_field_limit",
                    ConversionSeverity::Blocked,
                    "Conversion supports up to 64 fields per style and 1024 UTF-8 bytes per field.",
                    1,
                    None,
                    &style.name,
                );
                continue;
            }
            write_style(&mut output, &convert_style(style, variant, report));
        }
    }
    if let Some(script) = script {
        let inventory = script.attachments(200);
        if inventory.total > inventory.entries.len() {
            report.issue(
                "attachment_limit",
                ConversionSeverity::Blocked,
                "Cross-script conversion requires a complete inventory of at most 200 attachments.",
                1,
                None,
                "Use SRT to deliberately discard attachments, or reduce the inventory first.",
            );
        }
        for attachment in &inventory.entries {
            checkpoint()?;
            if let Some(problem) = attachment.problem {
                report.issue(
                    "attachment_boundary",
                    ConversionSeverity::Blocked,
                    "An attachment has an ambiguous or unsupported record boundary.",
                    1,
                    None,
                    &format!("{}: {problem}", attachment.name),
                );
                continue;
            }
            output.push_str(match attachment.kind {
                AttachmentKind::Font => "\n[Fonts]\nfontname: ",
                AttachmentKind::Graphic => "\n[Graphics]\nfilename: ",
            });
            output.push_str(&attachment.name);
            output.push('\n');
            script.read_attachment(attachment.id, |line| {
                checkpoint()?;
                output.push_str(line);
                output.push('\n');
                Ok(())
            })?;
        }
        report.issue("attachments_retained", ConversionSeverity::Notice, "Encoded font/graphic payloads are retained without decoding, rewriting or installing them. Destination-player attachment support can differ.", inventory.total as u32, None, "This conversion does not validate original font internals.");
    }
    output.push_str(if variant == Variant::Ass {
        "\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    } else {
        "\n[Events]\nFormat: Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
    });
    Ok(output)
}

fn write_style(output: &mut String, style: &Style) {
    output.push_str("Format: ");
    output.push_str(
        &style
            .fields
            .iter()
            .map(|(n, _)| n.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    );
    output.push_str("\nStyle: ");
    output.push_str(
        &style
            .fields
            .iter()
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push('\n');
}
fn field<'a>(style: &'a Style, name: &str) -> Option<&'a str> {
    style
        .fields
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}
fn color(value: &str) -> Result<u32> {
    if let Some(hex) = value
        .strip_prefix("&H")
        .or_else(|| value.strip_prefix("&h"))
    {
        u32::from_str_radix(hex.strip_suffix('&').unwrap_or(hex), 16)
            .map_err(|_| "Invalid color".into())
    } else {
        let number: i64 = value.parse().map_err(|_| "Invalid color")?;
        if !(-2147483648..=4294967295).contains(&number) {
            return Err("Invalid color".into());
        }
        Ok(number as u32)
    }
}
fn convert_style(style: &Style, target: Variant, report: &mut ConversionReport) -> Style {
    let mut next = default_style(target);
    for (name, value) in &mut next.fields {
        if let Some(original) = field(style, name) {
            *value = original.into();
        } else if !matches!(
            name.as_str(),
            "TertiaryColour" | "AlphaLevel" | "OutlineColour"
        ) {
            report.issue("style_defaults", ConversionSeverity::Loss, "Missing style fields receive explicit target defaults instead of renderer-dependent values.", 1, None, &format!("{}.{} = {value}", style.name, name));
        }
    }
    next.name = style.name.clone();
    for (name, value) in &style.fields {
        let handled = if target == Variant::Ass {
            name.eq_ignore_ascii_case("AlphaLevel") || name.eq_ignore_ascii_case("BackColour")
        } else {
            name.eq_ignore_ascii_case("OutlineColour")
        };
        if !handled
            && !next
                .fields
                .iter()
                .any(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            report.issue(
                "style_fields_removed",
                ConversionSeverity::Loss,
                "Style fields outside the destination schema are omitted.",
                1,
                None,
                &format!("{}.{} = {}", style.name, name, value.trim()),
            );
        }
    }
    let mapped = (|| -> Result<Vec<(&str, String)>> {
        // SSA's historical alignment numbers differ from ASS numpad numbering.
        let alignment: usize = field(style, "Alignment")
            .unwrap_or("2")
            .parse()
            .map_err(|_| "Invalid alignment")?;
        let ass_to_ssa = [0, 1, 2, 3, 9, 10, 11, 5, 6, 7];
        let alignment = if target == Variant::Ssa {
            *ass_to_ssa
                .get(alignment)
                .filter(|&&n| n != 0)
                .ok_or("Unsupported alignment")?
        } else {
            ass_to_ssa
                .iter()
                .position(|&n| n == alignment)
                .filter(|&n| n != 0)
                .ok_or("Unsupported SSA alignment")?
        };
        let primary = color(field(style, "PrimaryColour").unwrap_or("16777215"))?;
        let secondary = color(field(style, "SecondaryColour").unwrap_or("255"))?;
        let back = color(field(style, "BackColour").unwrap_or("0"))?;
        let mut result = vec![("Alignment", alignment.to_string())];
        if target == Variant::Ass {
            let alpha: u8 = field(style, "AlphaLevel")
                .unwrap_or("0")
                .parse()
                .map_err(|_| "Invalid SSA AlphaLevel")?;
            for (name, rgb, a) in [
                ("PrimaryColour", primary, alpha),
                ("SecondaryColour", secondary, alpha),
                ("OutlineColour", back, alpha),
                ("BackColour", back, 128),
            ] {
                result.push((
                    name,
                    format!("&H{:08X}", (rgb & 0xffffff) | ((a as u32) << 24)),
                ));
            }
            report.issue("ssa_style_mapping", ConversionSeverity::Notice, "SSA alignment, shared alpha and outline/shadow colors are mapped to explicit ASS fields using libass/VSFilter-compatible SSA semantics.", 1, None, &style.name);
        } else {
            let outline = color(field(style, "OutlineColour").unwrap_or("0"))?;
            result.extend([
                ("PrimaryColour", (primary & 0xffffff).to_string()),
                ("SecondaryColour", (secondary & 0xffffff).to_string()),
                ("BackColour", (outline & 0xffffff).to_string()),
                ("TertiaryColour", "0".into()),
                ("AlphaLevel", (primary >> 24).to_string()),
            ]);
            if secondary >> 24 != primary >> 24
                || outline >> 24 != primary >> 24
                || back & 0xffffff != outline & 0xffffff
                || back >> 24 != 128
            {
                report.issue("ssa_color_loss", ConversionSeverity::Loss, "SSA shares primary alpha across primary/secondary/outline, uses outline RGB for shadow and uses shadow alpha 128. Independent ASS colors/alphas cannot all survive.", 1, None, &style.name);
            }
        }
        Ok(result)
    })();
    match mapped {
        Ok(fields) => {
            for (name, value) in fields {
                next.fields
                    .iter_mut()
                    .find(|(n, _)| n == name)
                    .expect("canonical target field")
                    .1 = value;
            }
        }
        Err(error) => report.issue(
            "style_mapping",
            ConversionSeverity::Blocked,
            "A style color, alpha or alignment cannot be safely translated.",
            1,
            None,
            &format!("{}: {error}", style.name),
        ),
    }
    next
}

pub(super) fn discard_metadata(cue: &Cue, report: &mut ConversionReport) {
    if let Some(metadata) = cue.metadata() {
        for (name, value) in metadata.fields() {
            report.issue("event_fields_removed", ConversionSeverity::Loss, "SRT cannot store event style assignment, layer, actor, margins, effects or custom event fields.", 1, Some(cue), &format!("{name} = {}", value.trim()));
        }
    }
}
pub(super) fn metadata(
    cue: &Cue,
    target: ConversionFormat,
    report: &mut ConversionReport,
) -> Result<CueMetadata> {
    let get = |name: &str, fallback: &str| {
        cue.metadata()
            .and_then(|m| m.get(name))
            .map_or_else(|| fallback.to_owned(), |v| v.trim().to_owned())
    };
    let fields: Vec<(String, String)> = vec![
        (
            if target == ConversionFormat::Ass {
                "Layer"
            } else {
                "Marked"
            }
            .into(),
            if target == ConversionFormat::Ass {
                get("Layer", "0")
            } else {
                "Marked=0".into()
            },
        ),
        ("Style".into(), get("Style", "Default")),
        ("Name".into(), get("Name", "")),
        ("MarginL".into(), get("MarginL", "0")),
        ("MarginR".into(), get("MarginR", "0")),
        ("MarginV".into(), get("MarginV", "0")),
        ("Effect".into(), get("Effect", "")),
    ];
    if let Some(metadata) = cue.metadata() {
        for (name, value) in metadata.fields() {
            if !fields.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
                report.issue("event_fields_removed", ConversionSeverity::Loss, "Event fields outside the destination schema are omitted; ASS layering and SSA Marked state do not have equivalent meanings.", 1, Some(cue), &format!("{name} = {}", value.trim()));
            }
        }
    }
    // Stable canonical order is used by the target serializer, not map iteration.
    CueMetadata::new(fields)
}

fn omitted_records(
    document: &Document,
    source: &Source,
    report: &mut ConversionReport,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<()> {
    let serialized = source.serialize(document)?;
    let mut section = String::new();
    for line in serialized.lines() {
        checkpoint()?;
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.is_empty() {
            continue;
        }
        let known_header = matches!(
            line.to_ascii_lowercase().as_str(),
            "[script info]"
                | "[v4+ styles]"
                | "[v4 styles]"
                | "[events]"
                | "[fonts]"
                | "[graphics]"
        );
        // Encoded attachment lines may themselves look like bracketed sections.
        if matches!(section.as_str(), "[fonts]" | "[graphics]") && !known_header {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line.to_ascii_lowercase();
            if known_header {
                continue;
            }
        }
        let key = line
            .split_once(':')
            .map(|(k, _)| k.trim().to_ascii_lowercase());
        let structured = match section.as_str() {
            "[script info]" => key.is_some() && !line.starts_with(';'),
            "[v4 styles]" | "[v4+ styles]" => matches!(key.as_deref(), Some("format" | "style")),
            "[events]" => matches!(key.as_deref(), Some("format" | "dialogue")),
            "[fonts]" | "[graphics]" => true,
            _ => false,
        };
        if !structured {
            report.issue("other_records_removed", ConversionSeverity::Loss, "Comments, non-Dialogue events, editor/private sections and unrecognized records are omitted from the canonical export.", 1, None, line);
        }
    }
    Ok(())
}
