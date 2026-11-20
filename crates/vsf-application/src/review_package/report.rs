use super::*;
use crate::comparison::{Comparison, ExtraSections, escape_html};
use std::{collections::BTreeMap, fmt::Write};
pub const OPEN_TAG: &str =
    r#"<script type="application/vnd.visualsubforge.review+json" id="vsf-review-package">"#;
pub(super) fn extract(page: &str) -> Result<&str> {
    let (_, rest) = page
        .split_once(OPEN_TAG)
        .ok_or("This report doesn't contain review data")?;
    if rest.contains(OPEN_TAG) {
        return Err("Multiple review data blocks".into());
    }
    let (data, _) = rest
        .split_once("</script>")
        .ok_or("Incomplete review data block")?;
    if data.contains('<') {
        return Err("Invalid unescaped review data block".into());
    }
    Ok(data)
}
pub fn sections(package: &Package, comparison: &Comparison, embed: bool) -> Result<ExtraSections> {
    let mut extra = ExtraSections::default();
    if embed {
        let data = String::from_utf8(package.write_bounded(check_report_size)?)
            .map_err(|e| e.to_string())?
            .replace('&', "\\u0026")
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        extra.head_html = format!("{OPEN_TAG}{data}</script>");
    }
    let format =
        |s: &Subtitle| Format::from_extension(&s.format).ok_or("Unknown review subtitle format");
    let positions = reanchor(
        &package.threads,
        comparison,
        format(&package.original)?,
        format(&package.revised)?,
        || Ok(()),
    )?;
    let mut occurrences = BTreeMap::new();
    let ids: Vec<_> = comparison
        .changes
        .iter()
        .map(|c| {
            let n = occurrences.entry(&c.key).or_insert(0);
            let id = if *n == 0 {
                format!("change-{}", c.key)
            } else {
                format!("change-{}-{n}", c.key)
            };
            *n += 1;
            id
        })
        .collect();
    let open = package.threads.iter().filter(|t| !t.resolved()).count();
    extra.end_html = format!(
        "<section><h2>Discussion</h2><p>{open} open threads · {} threads in total</p></section>",
        package.threads.len()
    );
    let mut detached = String::new();
    for (t, pos) in package.threads.iter().zip(positions) {
        let out = match pos {
            Some(i) => extra.change_html.entry(ids[i].clone()).or_default(),
            None => &mut detached,
        };
        let _ = write!(
            out,
            "<section><h3>{} thread</h3>",
            if t.resolved() { "Resolved" } else { "Open" }
        );
        if pos.is_none() {
            let (a, b) = t.anchor.sides();
            for (label, side) in [("Original", a), ("Revised", b)] {
                for c in side {
                    let _ = write!(
                        out,
                        "<pre>{label} ({}–{} ms): {}</pre>",
                        c.start_ms,
                        c.end_ms,
                        escape_html(&c.text)
                    );
                }
            }
        }
        for m in &t.messages {
            let timestamp =
                time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(m.at_ms) * 1_000_000)
                    .ok()
                    .and_then(|t| {
                        t.format(&time::format_description::well_known::Rfc3339)
                            .ok()
                    })
                    .unwrap_or_else(|| format!("{} ms since the Unix epoch", m.at_ms));
            let _ = write!(
                out,
                "<p>{} · {timestamp}</p><pre>{}</pre>",
                escape_html(&m.author),
                escape_html(m.text.as_deref().unwrap_or(if m.event == Event::Resolve {
                    "Resolved"
                } else {
                    "Reopened"
                }))
            );
        }
        out.push_str("</section>");
    }
    if !detached.is_empty() {
        extra.end_html.push_str("<h2>Detached</h2>");
        extra.end_html.push_str(&detached)
    }
    Ok(extra)
}
