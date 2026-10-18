use super::*;
use std::collections::HashSet;

struct Setting {
    section: String,
    key: String,
    value: String,
    line: usize,
}

fn decode(bytes: &[u8]) -> Result<(String, Encoding)> {
    if bytes.len() > MAX_BYTES {
        return Err("Legacy project exceeds the 256 KiB limit".into());
    }
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        return Err(
            "UTF-32 projects are unsupported; convert a copy to UTF-8 or UTF-16 with a BOM".into(),
        );
    }
    let encoding = if bytes.starts_with(&[0xff, 0xfe]) {
        Encoding::Utf16Le
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        Encoding::Utf16Be
    } else {
        Encoding::Utf8
    };
    let text = match encoding {
        Encoding::Utf8 => {
            let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
            std::str::from_utf8(bytes)
                .map_err(|_| {
                    "Legacy project encoding is not UTF-8. Convert a copy from its original code page to UTF-8 or BOM-marked UTF-16"
                })?
                .to_owned()
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if !bytes.len().is_multiple_of(2) {
                return Err("Truncated UTF-16 legacy project".into());
            }
            let words: Vec<_> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| {
                    if encoding == Encoding::Utf16Le {
                        u16::from_le_bytes(pair)
                    } else {
                        u16::from_be_bytes(pair)
                    }
                })
                .collect();
            String::from_utf16(&words).map_err(
                |_| "Invalid UTF-16 legacy project; repair its encoding before importing",
            )?
        }
    };
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
    {
        return Err(
            "Legacy project contains NUL or unsupported control characters; verify its encoding"
                .into(),
        );
    }
    Ok((text, encoding))
}

fn settings(text: &str) -> Result<Vec<Setting>> {
    let mut section = String::new();
    let mut saw_project = false;
    let mut seen = HashSet::new();
    let mut settings = Vec::new();
    // Treat CRLF as one line; standalone CR is also supported without rewriting paths.
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for (index, raw) in normalized.lines().enumerate() {
        let line = index + 1;
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with([';', '#']) {
            continue;
        }
        if raw.starts_with('[') {
            let name = raw
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .ok_or_else(|| format!("Invalid INI section on line {line}"))?
                .trim();
            if name.is_empty()
                || name.chars().count() > 128
                || name.contains(['[', ']'])
                || name.chars().any(char::is_control)
            {
                return Err(format!("Invalid INI section on line {line}"));
            }
            section = name.to_lowercase();
            saw_project |= section == "visualsubsync";
            continue;
        }
        if section.is_empty() {
            return Err(format!("Setting before an INI section on line {line}"));
        }
        let (key, value) = raw
            .split_once('=')
            .ok_or_else(|| format!("Expected key=value on line {line}"))?;
        let key = key.trim();
        if key.is_empty() || key.chars().count() > 128 || key.chars().any(char::is_control) {
            return Err(format!("Invalid setting name on line {line}"));
        }
        let key = key.to_lowercase();
        if !seen.insert((section.clone(), key.clone())) {
            return Err(format!(
                "Duplicate setting [{section}] {key} on line {line}; remove the ambiguity in a copy"
            ));
        }
        let value = value.trim();
        // Windows INI strings can be enclosed in matching single/double quotes.
        // Inline semicolons, hashes and equals signs remain part of the value.
        let value = if value.len() >= 2
            && (value.starts_with('"') && value.ends_with('"')
                || value.starts_with('\'') && value.ends_with('\''))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        if value.chars().count() > MAX_VALUE_CHARS || value.chars().any(char::is_control) {
            return Err(format!(
                "Setting on line {line} exceeds 4096 characters or contains control characters"
            ));
        }
        if settings.len() == MAX_SETTINGS {
            return Err("Legacy project exceeds 512 settings".into());
        }
        settings.push(Setting {
            section: section.clone(),
            key,
            value: value.into(),
            line,
        });
    }
    if !saw_project {
        return Err("Missing [VisualSubsync] section; choose a VisualSubSync project".into());
    }
    Ok(settings)
}

fn warn(warnings: &mut Vec<Warning>, setting: &Setting, message: &str) {
    warnings.push(Warning {
        field: format!("[{}] {}", setting.section, setting.key),
        line: Some(setting.line),
        value: Some(setting.value.clone()),
        message: message.into(),
    });
}

fn time(setting: Option<&Setting>, warnings: &mut Vec<Warning>) -> Option<u32> {
    let setting = setting?;
    if setting.value == "-1" {
        return None;
    }
    match setting.value.parse::<i32>() {
        Ok(value) if value >= 0 => Some(value as u32),
        _ => {
            warn(
                warnings,
                setting,
                "Invalid legacy millisecond position; use the default instead",
            );
            None
        }
    }
}

/// Parse independently authored INI handling based on the documented legacy
/// fields. No script, dictionary, preset or native code is loaded or executed.
pub fn parse(bytes: &[u8]) -> Result<LegacyProject> {
    let (text, encoding) = decode(bytes)?;
    let settings = settings(&text)?;
    let mut project = LegacyProject {
        encoding,
        references: Vec::new(),
        view: View::default(),
        warnings: Vec::new(),
    };
    for setting in &settings {
        if setting.section != "visualsubsync" {
            warn(
                &mut project.warnings,
                setting,
                "This section/setting is not imported",
            );
            continue;
        }
        let kind = match setting.key.as_str() {
            "videosource" => Some(ReferenceKind::Video),
            "wavfile" => Some(ReferenceKind::Audio),
            "peakfile" => Some(ReferenceKind::Peaks),
            "subtitlesfile" => Some(ReferenceKind::Subtitles),
            "subtitlesvo" => Some(ReferenceKind::ReferenceSubtitles),
            "textpipesource" => Some(ReferenceKind::TextPipe),
            _ => None,
        };
        if let Some(kind) = kind
            && !setting.value.is_empty()
        {
            project.references.push(Reference {
                kind,
                value: setting.value.clone(),
                line: setting.line,
            });
        }
        let message = match setting.key.as_str() {
            "videosource"
            | "wavfile"
            | "subtitlesfile"
            | "focusedtimems"
            | "wavdisplayerpositionstartms"
            | "wavdisplayerpositionstopms" => None,
            "wavmode" => Some(if matches!(setting.value.as_str(), "0" | "1" | "2") {
                "Legacy waveform mode is not imported; VSF generates waveform data from the selected media"
            } else {
                "Invalid legacy waveform mode; VSF generates waveform data from the selected media"
            }),
            "peakfile" => Some(
                "Legacy peak caches are not imported; waveform data is generated from the selected media",
            ),
            "subtitlesvo" => Some(
                "Reference subtitles are not attached by this import; the reference path remains available in this review",
            ),
            "textpipesource" | "textpipeposition" => {
                Some("Text-pipe/script contents and cursor positions are not imported or executed")
            }
            "dictionary" => Some(
                "Legacy dictionary names are not mapped; select English or French in VSF settings",
            ),
            "presets" => Some(
                "Legacy presets/plugins are not loaded; import a supported timing or quality profile separately",
            ),
            "showvideo"
            | "showvo"
            | "detachedvideo"
            | "videopanelwidth"
            | "videopanelheight"
            | "videowindownormalleft"
            | "videowindownormaltop"
            | "videowindownormalwidth"
            | "videowindownormalheight" => {
                Some("Legacy window geometry and display flags are not imported")
            }
            _ => Some("This setting is not imported"),
        };
        if let Some(message) = message
            && (!setting.value.is_empty() || kind.is_none())
        {
            warn(&mut project.warnings, setting, message);
        }
    }
    let get = |key: &str| {
        settings
            .iter()
            .find(|s| s.section == "visualsubsync" && s.key == key)
    };
    let start = time(get("wavdisplayerpositionstartms"), &mut project.warnings);
    let stop = time(get("wavdisplayerpositionstopms"), &mut project.warnings);
    match (start, stop) {
        (Some(start), Some(stop)) if stop > start => {
            project.view.start_ms = start;
            project.view.span_ms = stop - start;
        }
        (None, None) => {}
        _ => project.warnings.push(Warning {
            field: "Waveform interval".into(),
            line: None,
            value: None,
            message: "Incomplete or reversed waveform interval; use the default 0–15 seconds"
                .into(),
        }),
    }
    project.view.focused_ms = time(get("focusedtimems"), &mut project.warnings);
    if project.view.focused_ms.is_some()
        && let Some(setting) = get("focusedtimems")
    {
        warn(
            &mut project.warnings,
            setting,
            "Use the legacy focused cue time as the paused resume position; exact cue selection is not imported",
        );
    }
    if project.reference(ReferenceKind::Video).is_some()
        && project.reference(ReferenceKind::Audio).is_some()
        && let Some(setting) = get("wavfile")
    {
        warn(
            &mut project.warnings,
            setting,
            "Separate external audio is not attached; playback and waveform use the selected media's audio",
        );
    }
    Ok(project)
}
