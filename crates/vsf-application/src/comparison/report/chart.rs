use super::*;

fn clock(ms: u64) -> String {
    format!("{:02}:{:02}", ms / 60000, ms / 1000 % 60)
}
pub(super) fn render(
    out: &mut String,
    c: &Comparison,
    ids: &[String],
    extra: &ExtraSections,
    include_unchanged: bool,
) {
    let Overview { width_ms, buckets } = overview(&c.changes);
    let n = buckets.len();
    let max = buckets
        .iter()
        .map(|b| b.total)
        .max()
        .unwrap_or(1)
        .max(1)
        .div_ceil(5)
        * 5;
    let _ = write!(
        out,
        "<section class=\"card\"><div class=\"card-head\"><h2>Where the changes are</h2><p>Each bar is {} seconds. Grey is every cue, colour is changed cues by kind. Select a bar to go to its first change.</p></div><div class=\"chart\" style=\"--buckets:{n}\"><div class=\"yaxis\"><span>{max}</span><span>0</span></div><svg viewBox=\"0 0 {} 100\" preserveAspectRatio=\"none\" role=\"group\" aria-label=\"Changed cues over time\">",
        width_ms as f64 / 1000.,
        n * 10
    );
    for (i, b) in buckets.iter().enumerate() {
        let x = i * 10 + 1;
        if let Some(first) = b.first {
            let _ = write!(out, "<a href=\"#{}\">", ids[first as usize]);
        } else {
            out.push_str("<g>");
        }
        let changed: u32 = b.kinds.iter().sum();
        let _ = write!(
            out,
            "<title>{}–{} · {changed} of {} cues changed",
            clock(i as u64 * width_ms),
            clock((i as u64 + 1) * width_ms),
            b.total
        );
        if let Some(first) = b.first {
            let row = &c.changes[first as usize];
            if let Some(q) = row.revised.first().or(row.original.first()) {
                let preview: String = q.text.chars().take(80).collect();
                let _ = write!(out, " · {}", escape_html(&preview));
            }
        }
        out.push_str("</title>");
        let mut y = 100.;
        for (cls, count) in std::iter::once(("all".to_string(), b.total)).chain(
            KINDS
                .iter()
                .zip(b.kinds)
                .filter(|(_, v)| *v > 0)
                .map(|((k, _), v)| (format!("k-{k}"), v)),
        ) {
            let h = f64::from(count) * 100. / f64::from(max);
            let top = y - h;
            let _ = write!(
                out,
                "<rect class=\"{cls}\" x=\"{x}\" y=\"{top:.2}\" width=\"8\" height=\"{h:.2}\"/>"
            );
            if cls != "all" {
                y = top;
            }
        }
        out.push_str(if b.first.is_some() { "</a>" } else { "</g>" });
    }
    out.push_str("</svg><div class=\"pins\">");
    let mut pins = vec![None; n];
    for (c, id) in c.changes.iter().zip(ids) {
        if let Some(count) = extra.open_rows.get(id)
            && let Some(q) = c.revised.first().or(c.original.first())
        {
            let (_, total) = pins[(q.start_ms / width_ms) as usize].get_or_insert_with(|| {
                (
                    if include_unchanged && c.kind == Kind::Unchanged {
                        format!("discussion-{id}")
                    } else {
                        id.clone()
                    },
                    0usize,
                )
            });
            *total += count;
        }
    }
    for pin in pins {
        if let Some((id, count)) = pin {
            let _ = write!(
                out,
                "<a class=\"pin\" href=\"#{id}\" title=\"{count} open thread{}\"></a>",
                if count == 1 { "" } else { "s" }
            );
        } else {
            out.push_str("<span></span>");
        }
    }
    let _ = write!(
        out,
        "</div><div class=\"axis\"><span>00:00</span><span class=\"end\">{}</span></div></div><ul class=\"legend\">",
        clock(n as u64 * width_ms)
    );
    for (k, label) in [("all", "All cues")].into_iter().chain(KINDS) {
        let _ = write!(out, "<li><span class=\"sw k-{k}\"></span>{label}</li>");
    }
    out.push_str("<li><span class=\"sw pin\"></span>Open thread</li></ul></section>");
}
