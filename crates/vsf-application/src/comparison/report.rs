use super::*;
use std::{collections::BTreeMap, fmt::Write};
pub struct ReportOptions<'a> {
    pub original_name: &'a str,
    pub revised_name: &'a str,
    /// Caller-formatted date, escaped just like names and subtitle content.
    pub date: &'a str,
    pub include_unchanged: bool,
}
/// Trusted, already-encoded extension fragments, never raw subtitle/user text.
/// Later package writers can supply an inert data block and escaped thread HTML.
#[derive(Default)]
pub struct ExtraSections {
    pub head_html: String,
    pub change_html: BTreeMap<String, String>,
    pub end_html: String,
}
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn timecode(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3600000,
        ms / 60000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}
fn label(kind: Kind) -> &'static str {
    match kind {
        Kind::TextAndTiming => "Text and timing",
        Kind::Unchanged => "Unchanged",
        Kind::Added => "Added",
        Kind::Removed => "Removed",
        Kind::Split => "Split",
        Kind::Merged => "Merged",
        Kind::Timing => "Timing",
        Kind::Style => "Style",
        Kind::Text => "Text",
    }
}
pub fn html(comparison: &Comparison, options: &ReportOptions<'_>, extra: &ExtraSections) -> String {
    let mut out = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Subtitle comparison</title><style>body{font:16px system-ui;max-width:72rem;margin:2rem auto;padding:1rem;color:#17212b;background:#fff}article{border-top:1px solid #aaa;padding:1rem 0}pre,.diff{white-space:pre-wrap;overflow-wrap:anywhere}del{background:#ffe0e0}ins{background:#d9f5dc}small{display:block}</style>",
    );
    out.push_str(&extra.head_html);
    out.push_str("</head><body><h1>Subtitle comparison</h1>");
    let s = &comparison.statistics;
    let _ = write!(
        out,
        "<p>{} → {} · {}</p><p>{} change{} · text {:.1}% · timing {:.1}% · total {:.1}% · {} added cue{}</p><pre>{}</pre>",
        escape_html(options.original_name),
        escape_html(options.revised_name),
        escape_html(options.date),
        s.changes,
        if s.changes == 1 { "" } else { "s" },
        s.text_percent,
        s.timing_percent,
        s.total_percent,
        s.added_cues,
        if s.added_cues == 1 { "" } else { "s" },
        escape_html(&serde_json::to_string(&comparison.options).expect("options serialize"))
    );
    let mut occurrences = BTreeMap::new();
    let mut blocks = comparison.blocks.iter().peekable();
    for (index, c) in comparison.changes.iter().enumerate() {
        if let Some(block) = blocks.peek()
            && index == block.first as usize
        {
            let _ = write!(
                out,
                "<details><summary>Shifted {} cues by {:+.3} s ({} – {})</summary>",
                block.end - block.first,
                block.delta_ms as f64 / 1000.0,
                timecode(block.revised_start_ms),
                timecode(block.revised_end_ms)
            );
        }
        if c.kind == Kind::Unchanged && !options.include_unchanged {
            continue;
        }
        let occurrence = occurrences.entry(&c.key).or_insert(0usize);
        let id = if *occurrence == 0 {
            c.key.clone()
        } else {
            format!("{}-{}", c.key, occurrence)
        };
        *occurrence += 1;
        let _ = write!(
            out,
            "<article id=\"change-{}\"><h2>{}</h2><div class=\"diff\">",
            escape_html(&id),
            label(c.kind)
        );
        for token in &c.words {
            let text = escape_html(&token.text).replace('\n', "↵\n");
            match token.kind {
                TokenKind::Equal => out.push_str(&text),
                TokenKind::Removed => {
                    let _ = write!(out, "<del>{text}</del>");
                }
                TokenKind::Added => {
                    let _ = write!(out, "<ins>{text}</ins>");
                }
            }
        }
        out.push_str("</div>");
        for (label, side) in [("Original", &c.original), ("Revised", &c.revised)] {
            for cue in side {
                let rate = |r: &metrics::Rate| {
                    r.value()
                        .map_or_else(|| "unavailable".into(), |v| format!("{v:.2}"))
                };
                let _ = write!(
                    out,
                    "<small>{label} #{}: {}–{} · {} characters · CPS {} · RS {} · style {} · actor {}</small>",
                    cue.source_index + 1,
                    timecode(cue.start_ms),
                    timecode(cue.end_ms),
                    cue.characters,
                    rate(&cue.cps),
                    rate(&cue.reading_speed),
                    escape_html(cue.style.as_deref().unwrap_or("")),
                    escape_html(cue.actor.as_deref().unwrap_or(""))
                );
            }
        }
        if matches!(c.kind, Kind::Split | Kind::Merged) {
            if let (Some((a, b)), Some((x, y))) = (span(&c.original), span(&c.revised)) {
                let _ = write!(
                    out,
                    "<small>Group span Δ in {:+} ms · out {:+} ms</small>",
                    x as i64 - a as i64,
                    y as i64 - b as i64
                );
            }
        } else if let ([original], [revised]) = (c.original.as_slice(), c.revised.as_slice()) {
            let delta = |a: &metrics::Rate, b: &metrics::Rate| match (a.value(), b.value()) {
                (Some(a), Some(b)) => format!("{:+.2}", b - a),
                _ => "unavailable".into(),
            };
            let _ = write!(
                out,
                "<small>Δ in {:+} ms · out {:+} ms · characters {:+} · CPS {} · RS {}</small>",
                i128::from(revised.start_ms) - i128::from(original.start_ms),
                i128::from(revised.end_ms) - i128::from(original.end_ms),
                i128::from(revised.characters) - i128::from(original.characters),
                delta(&original.cps, &revised.cps),
                delta(&original.reading_speed, &revised.reading_speed)
            );
        }
        if let Some(extra) = extra.change_html.get(&format!("change-{id}")) {
            out.push_str(extra);
        }
        out.push_str("</article>");
        if blocks
            .peek()
            .is_some_and(|block| index + 1 == block.end as usize)
        {
            out.push_str("</details>");
            blocks.next();
        }
    }
    out.push_str(&extra.end_html);
    out.push_str("</body></html>\n");
    out
}
