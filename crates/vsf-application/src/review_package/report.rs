use super::*;
use crate::comparison::{Comparison, ExtraSections, escape_html};
use std::{collections::BTreeSet, fmt::Write};
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
pub fn sections(
    package: &Package,
    comparison: &Comparison,
    embed: bool,
    offset_minutes: i16,
) -> Result<ExtraSections> {
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
    let original_format = format(&package.original)?;
    let revised_format = format(&package.revised)?;
    let positions = reanchor(
        &package.threads,
        comparison,
        original_format,
        revised_format,
        || Ok(()),
    )?;
    let ids = crate::comparison::change_ids(&comparison.changes);
    extra.open = package.threads.iter().filter(|t| !t.resolved()).count();
    extra.total = package.threads.len();
    extra.author = package.exported_by.clone();
    extra.round = Some(package.round);
    let mut detached = String::new();
    for (n, (thread, pos)) in package.threads.iter().zip(positions).enumerate() {
        let out = if let Some(i) = pos {
            if !thread.resolved() {
                *extra.open_rows.entry(ids[i].clone()).or_default() += 1;
            }
            extra.change_html.entry(ids[i].clone()).or_default()
        } else {
            detached.push_str("<article class=\"detached-card\"><div class=\"pair\">");
            let (a, b) = thread.anchor.sides();
            for (label, cues, fmt) in [
                ("Original", a, original_format),
                ("Revised", b, revised_format),
            ] {
                let _ = write!(detached, "<div><span class=\"label\">{label}</span>");
                for q in cues {
                    let _ = write!(
                        detached,
                        "<p>{}–{} · {}</p>",
                        crate::comparison::timecode(q.start_ms, fmt),
                        crate::comparison::timecode(q.end_ms, fmt),
                        escape_html(&q.text)
                    );
                }
                detached.push_str("</div>");
            }
            detached.push_str("</div>");
            &mut detached
        };
        let status = if thread.resolved() {
            "Resolved"
        } else {
            "Open"
        };
        let count = thread
            .messages
            .iter()
            .filter(|m| m.event == Event::Comment)
            .count();
        let people = thread
            .messages
            .iter()
            .map(|m| m.author.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(escape_html)
            .collect::<Vec<_>>()
            .join(", ");
        let id = n + 1;
        let _ = write!(
            out,
            "<div class=\"thread {}\"><input type=\"checkbox\" class=\"vh tt\" id=\"t-{id}\"><label class=\"disc\" for=\"t-{id}\"><svg viewBox=\"0 0 16 16\" aria-hidden=\"true\"><path d=\"M2 3h12v8H7l-3 3v-3H2z\"/></svg><span>{count} comment{}</span><span class=\"status\">{status}</span><span class=\"people\">{people}</span><span class=\"chev\"></span></label><ol class=\"thread-body\">",
            status.to_lowercase(),
            if count == 1 { "" } else { "s" }
        );
        for m in &thread.messages {
            let at = local_date(m.at_ms, offset_minutes)?;
            let _ = write!(
                out,
                "<li><span class=\"who a{}\">{}</span> ",
                author_colour(&m.author),
                escape_html(&m.author)
            );
            out.push_str(match m.event {
                Event::Resolve => "resolved this thread ",
                Event::Reopen => "reopened this thread ",
                Event::Comment => "",
            });
            let _ = write!(out, "<time>{at}</time>");
            if let Some(text) = &m.text {
                let _ = write!(out, "<p>{}</p>", escape_html(text));
            }
            out.push_str("</li>");
        }
        out.push_str("</ol></div>");
        if pos.is_none() {
            out.push_str("</article>");
        }
    }
    if !detached.is_empty() {
        extra.end_html =
            format!("<section class=\"detached\"><h2>Detached</h2>{detached}</section>");
    }
    Ok(extra)
}
fn author_colour(name: &str) -> u32 {
    name.bytes().map(u32::from).sum::<u32>() % 6
}
fn local_date(ms: u64, offset_minutes: i16) -> Result<String> {
    let offset = time::UtcOffset::from_whole_seconds(i32::from(offset_minutes) * 60)
        .map_err(|e| e.to_string())?;
    let Some(t) = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .ok()
        .and_then(|t| t.checked_to_offset(offset))
    else {
        return Ok(format!("{ms} ms since the Unix epoch"));
    };
    Ok(format!(
        "{} {} {} at {:02}:{:02}",
        t.day(),
        &t.month().to_string()[..3],
        t.year(),
        t.hour(),
        t.minute(),
    ))
}
pub fn report_date(ms: u64, offset_minutes: i16) -> Result<String> {
    Ok(format!(
        "{} UTC{}{:02}:{:02}",
        local_date(ms, offset_minutes)?,
        if offset_minutes < 0 { '−' } else { '+' },
        offset_minutes.unsigned_abs() / 60,
        offset_minutes.unsigned_abs() % 60
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn author_colours_use_utf8_bytes() {
        assert_eq!(author_colour("Ana"), 2);
        assert_eq!(author_colour("é"), 4);
    }
}
