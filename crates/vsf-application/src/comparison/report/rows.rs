use super::*;

pub(crate) fn timecode(ms: u64, format: Format) -> String {
    let (h, m, s) = (ms / 3600000, ms / 60000 % 60, ms / 1000 % 60);
    match format {
        Format::Ass | Format::Ssa => format!("{h}:{m:02}:{s:02}.{:02}", ms % 1000 / 10),
        Format::Srt => format!("{h:02}:{m:02}:{s:02},{:03}", ms % 1000),
        Format::WebVtt => format!("{h:02}:{m:02}:{s:02}.{:03}", ms % 1000),
    }
}
fn text(out: &mut String, value: &str, kind: TokenKind) {
    let tag = match kind {
        TokenKind::Equal => {
            out.push_str(&escape_html(value));
            return;
        }
        TokenKind::Removed => "del",
        TokenKind::Added => "ins",
    };
    for (i, line) in value.split('\n').enumerate() {
        if i > 0 {
            let _ = writeln!(out, "<{tag} class=\"br\">↵</{tag}>");
        }
        if !line.is_empty() {
            let _ = write!(out, "<{tag}>{}</{tag}>", escape_html(line));
        }
    }
}
fn side(out: &mut String, c: &Change, revised: bool, format: Format) {
    let (cues, other, cls, skip, empty) = if revised {
        (
            &c.revised,
            &c.original,
            "rev",
            TokenKind::Removed,
            "Removed",
        )
    } else {
        (
            &c.original,
            &c.revised,
            "orig",
            TokenKind::Added,
            "Not in the original",
        )
    };
    let _ = write!(out, "<div class=\"side {cls}\">");
    if cues.is_empty() {
        let _ = write!(out, "<span class=\"none\">{empty}</span>");
    }
    let grouped = matches!(c.kind, Kind::Merged | Kind::Split);
    // The comparison joins cue texts with a single newline. Consume that separator
    // between numbered rows without mistaking a newline inside a cue for a boundary.
    let mut tokens = c
        .words
        .iter()
        .filter(|t| t.kind != skip)
        .flat_map(|t| t.text.split_inclusive('\n').map(move |s| (s, t.kind)));
    let mut rest = tokens.next();
    for (index, q) in cues.iter().enumerate() {
        if index > 0
            && let Some((s, k)) = rest
        {
            rest = if s.len() == 1 {
                tokens.next()
            } else {
                Some((&s[1..], k))
            };
        }
        if grouped {
            let _ = write!(
                out,
                "<div class=\"cue\"><span class=\"cn\">#{}</span><div class=\"tx\">",
                q.source_index + 1
            );
        }
        if c.timing_changed {
            out.push_str("<span class=\"times\">");
            for (i, t) in [q.start_ms, q.end_ms].into_iter().enumerate() {
                if i == 1 {
                    out.push_str(" → ");
                }
                let value = timecode(t, format);
                if revised
                    && other
                        .get(index)
                        .is_none_or(|o| [o.start_ms, o.end_ms][i] != t)
                {
                    let _ = write!(out, "<span class=\"tc\"><ins>{value}</ins></span>");
                } else {
                    let _ = write!(out, "<span class=\"tc\">{value}</span>");
                }
            }
            out.push_str("</span>");
        }
        let mut bytes = q.text.len();
        while let Some((s, k)) = rest
            && bytes > 0
        {
            let take = s.len().min(bytes);
            text(out, &s[..take], k);
            bytes -= take;
            rest = if take == s.len() {
                tokens.next()
            } else {
                Some((&s[take..], k))
            };
        }
        if c.style_changed {
            let _ = write!(
                out,
                "<small class=\"metrics\"> {} {}</small>",
                escape_html(q.style.as_deref().unwrap_or("")),
                escape_html(q.actor.as_deref().unwrap_or(""))
            );
        }
        if grouped {
            out.push_str("</div></div>");
        }
    }
    out.push_str("</div>");
}
pub(super) fn row(
    out: &mut String,
    c: &Change,
    id: &str,
    opts: &ReportOptions<'_>,
    extra: &ExtraSections,
) {
    let (class, label) = match c.kind {
        Kind::Text => ("k-text", "Text"),
        Kind::TextAndTiming => ("k-text k-timing", "Text and timing"),
        Kind::Timing => ("k-timing", "Timing"),
        Kind::Added => ("k-added", "Added"),
        Kind::Removed => ("k-removed", "Removed"),
        Kind::Merged => ("k-split", "Merged"),
        Kind::Split => ("k-split", "Split"),
        Kind::Style => ("k-style", "Style"),
        Kind::Unchanged => ("k-unchanged", "Unchanged"),
    };
    let open = if extra.open_rows.contains_key(id) {
        " has-open"
    } else {
        ""
    };
    let cue_numbers = if c.revised.is_empty() {
        &c.original
    } else {
        &c.revised
    }
    .iter()
    .map(|q| format!("#{}", q.source_index + 1))
    .collect::<Vec<_>>()
    .join(", ");
    let was = if c.revised.is_empty() { "was " } else { "" };
    let _ = write!(
        out,
        "<article class=\"change {class}{open}\" id=\"{id}\"><div class=\"meta\"><a class=\"num\" href=\"#{id}\">{was}{cue_numbers}</a><span class=\"kind {}\">{label}</span>",
        class.split_whitespace().next().unwrap_or(class)
    );
    if c.timing_changed
        && let (Some((a, b)), Some((x, y))) = (span(&c.original), span(&c.revised))
    {
        for (name, delta) in [
            ("in", x as i128 - a as i128),
            ("out", y as i128 - b as i128),
        ] {
            if delta != 0 {
                let _ = write!(
                    out,
                    "<span class=\"delta\">{name} {:+.3} s</span>",
                    delta as f64 / 1000.0
                );
            }
        }
    }
    out.push_str("</div>");
    side(out, c, false, opts.original_format);
    side(out, c, true, opts.revised_format);
    out.push_str("<div class=\"rs\">");
    let metrics = |cues: &[CueSnapshot]| {
        let rate = |r: &metrics::Rate| r.value().map_or_else(|| "—".into(), |v| format!("{v:.1}"));
        let join = |values: Vec<String>| values.join(" / ");
        [
            (
                join(cues.iter().map(|q| rate(&q.reading_speed)).collect()),
                "RS",
            ),
            (join(cues.iter().map(|q| rate(&q.cps)).collect()), "cps"),
            (
                join(cues.iter().map(|q| q.characters.to_string()).collect()),
                "chars",
            ),
        ]
    };
    let after = metrics(&c.revised);
    if !c.revised.is_empty() {
        let values = after
            .iter()
            .map(|(v, unit)| format!("{v} {unit}"))
            .collect::<Vec<_>>();
        let _ = write!(out, "<span class=\"metrics\">{}</span>", values.join(" · "));
    }
    if !c.original.is_empty() {
        let prior = metrics(&c.original)
            .into_iter()
            .zip(after)
            .filter_map(|((v, unit), (new, _))| (v != new).then(|| format!("{v} {unit}")))
            .collect::<Vec<_>>();
        if !prior.is_empty() {
            let _ = write!(
                out,
                "<span class=\"metrics\">was {}</span>",
                prior.join(" · ")
            );
        }
    }
    if let Some(transition) = c.band_change() {
        let _ = write!(out, "<span class=\"band-change\">{transition}</span>");
    }
    out.push_str("</div>");
    if c.kind != Kind::Unchanged
        && let Some(threads) = extra.change_html.get(id)
    {
        let _ = write!(out, "<div class=\"threads\">{threads}</div>");
    }
    out.push_str("</article>\n");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_format_clock_precision() {
        for (f, expected) in [
            (Format::Srt, "01:02:03,450"),
            (Format::WebVtt, "01:02:03.450"),
            (Format::Ass, "1:02:03.45"),
            (Format::Ssa, "1:02:03.45"),
        ] {
            assert_eq!(timecode(3723450, f), expected);
        }
    }
}
