use super::*;
use std::{collections::BTreeMap, fmt::Write};
mod chart;
mod rows;

pub struct ReportOptions<'a> {
    pub original_name: &'a str,
    pub revised_name: &'a str,
    pub original_format: Format,
    pub revised_format: Format,
    pub date: &'a str,
    pub include_unchanged: bool,
}
/// Trusted fragments are produced by the package renderer. All user content in
/// them has already passed through escape_html.
#[derive(Default)]
pub struct ExtraSections {
    pub head_html: String,
    pub change_html: BTreeMap<String, String>,
    pub end_html: String,
    pub open_rows: BTreeMap<String, usize>,
    pub open: usize,
    pub total: usize,
    pub author: String,
    pub round: Option<u32>,
}
pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub(crate) fn change_ids(changes: &[Change]) -> Vec<String> {
    let mut occurrences = BTreeMap::new();
    changes
        .iter()
        .map(|c| {
            let n = occurrences.entry(&c.key).or_insert(0);
            let key = escape_html(&c.key);
            let id = if *n == 0 {
                format!("change-{key}")
            } else {
                format!("change-{key}-{n}")
            };
            *n += 1;
            id
        })
        .collect()
}
const KINDS: [(&str, &str); 6] = [
    ("added", "Added"),
    ("removed", "Removed"),
    ("split", "Split or merged"),
    ("text", "Text"),
    ("style", "Style"),
    ("timing", "Timing"),
];
fn filters(out: &mut String, c: &Comparison, unchanged: bool) {
    out.push_str("<nav class=\"filters\" aria-label=\"Filter changes\"><h2>Changes</h2>");
    for i in [3, 5, 0, 1, 2, 4] {
        let (k, label) = KINDS[i];
        let count = c
            .changes
            .iter()
            .filter(|row| {
                matches!((row.kind, i), (Kind::TextAndTiming, 3 | 5))
                    || (row.kind != Kind::Timing && kind_index(row) == Some(i))
                    || (i == 5 && row.kind == Kind::Timing)
            })
            .count();
        let _ = write!(
            out,
            "<input type=\"checkbox\" class=\"vh\" id=\"f-{k}\" checked><label class=\"toggle\" for=\"f-{k}\"><span class=\"sw k-{k}\"></span>{label}<span class=\"count\">{count}</span></label>"
        );
    }
    if unchanged {
        out.push_str("<input type=\"checkbox\" class=\"vh\" id=\"f-unchanged\"><label class=\"toggle\" for=\"f-unchanged\">Unchanged</label>");
    }
    out.push_str("<input type=\"checkbox\" class=\"vh\" id=\"f-open\"><label class=\"toggle\" for=\"f-open\">Only with open threads</label><input type=\"checkbox\" class=\"vh\" id=\"f-expand\"><label class=\"toggle\" for=\"f-expand\"><span class=\"when-off\">Expand all comments</span><span class=\"when-on\">Collapse all comments</span></label></nav>");
}
pub fn html(c: &Comparison, opts: &ReportOptions<'_>, extra: &ExtraSections) -> String {
    let mut out = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Subtitle comparison</title><style>",
    );
    out.push_str(include_str!("report/report.css"));
    out.push_str("</style>");
    out.push_str(&extra.head_html);
    out.push_str("</head><body><main class=\"page\"><header><span class=\"eyebrow\">VisualSubForge · comparison report");
    if let Some(round) = extra.round {
        let _ = write!(out, " · round {round}");
    }
    let _ = write!(
        out,
        "</span><h1>{} → {}</h1><p class=\"sub\">Exported ",
        escape_html(opts.original_name),
        escape_html(opts.revised_name)
    );
    if !extra.author.is_empty() {
        let _ = write!(
            out,
            "by <strong>{}</strong> on ",
            escape_html(&extra.author)
        );
    }
    let s = &c.statistics;
    let _ = write!(
        out,
        "{} · {} original cues</p><ul class=\"chips\" aria-label=\"Comparison options\"><li>Timing tolerance {} ms</li>",
        escape_html(opts.date),
        s.original_cues,
        c.options.tolerance_ms
    );
    for (label, ignored) in [
        ("Tags", c.options.ignore_tags),
        ("Line breaks", c.options.ignore_line_breaks),
        ("Whitespace", c.options.ignore_whitespace),
    ] {
        let _ = write!(
            out,
            "<li>{label} {}</li>",
            if ignored { "ignored" } else { "compared" }
        );
    }
    out.push_str("</ul></header><dl class=\"tiles\" aria-label=\"Summary\">");
    let removed = c.changes.iter().filter(|q| q.kind == Kind::Removed).count();
    for (label, value) in [
        (
            format!("changed cues of {}", s.original_cues),
            s.changed_cues.to_string(),
        ),
        ("text changed".into(), format!("{:.1}%", s.text_percent)),
        ("timing changed".into(), format!("{:.1}%", s.timing_percent)),
        (
            "cues added / removed".into(),
            format!("+{} / −{removed}", s.added_cues),
        ),
        (
            format!(
                "of {} thread{}",
                extra.total,
                if extra.total == 1 { "" } else { "s" }
            ),
            format!(
                "{} open thread{}",
                extra.open,
                if extra.open == 1 { "" } else { "s" }
            ),
        ),
    ] {
        let _ = write!(out, "<div><dt>{label}</dt><dd>{value}</dd></div>");
    }
    out.push_str("</dl>");
    if s.changes == 0 {
        out.push_str("<section class=\"card empty\"><h2>No differences</h2>The two files have the same cues, text and timing with these options.</section>");
    }
    // Preserve all visible discussion, including comments on unchanged cues.
    if s.changes > 0 || extra.total > 0 {
        let ids = change_ids(&c.changes);
        if s.changes > 0 {
            chart::render(&mut out, c, &ids, extra, opts.include_unchanged);
        }
        filters(&mut out, c, opts.include_unchanged);
        out.push_str("<section id=\"changes\" aria-label=\"Cue changes\"><div class=\"cols\"><span>Cue</span><span>Original</span><span>Revised</span><span>Reading speed</span></div>");
        let mut blocks = c.blocks.iter().enumerate().peekable();
        let mut cue_discussion = String::new();
        for (i, (row, id)) in c.changes.iter().zip(&ids).enumerate() {
            if row.kind == Kind::Unchanged {
                if let Some(threads) = extra.change_html.get(id)
                    && let Some(q) = row.revised.first()
                {
                    let target = if opts.include_unchanged {
                        format!("discussion-{id}")
                    } else {
                        id.clone()
                    };
                    let _ = write!(
                        cue_discussion,
                        "<article class=\"detached-card\" id=\"{target}\"><strong>Cue #{}</strong><div class=\"side\">{}</div>{threads}</article>",
                        q.source_index + 1,
                        escape_html(&q.text)
                    );
                }
                if !opts.include_unchanged {
                    continue;
                }
            }
            if let Some(&(n, b)) = blocks.peek()
                && b.first as usize == i
            {
                let _ = write!(
                    out,
                    "<div class=\"shift k-timing\"><input type=\"checkbox\" class=\"vh sb\" id=\"b-{}\"><label class=\"shift-head\" for=\"b-{}\"><span class=\"chev\"></span><span class=\"kind k-timing\">Timing</span><span>Shifted {} cues by <strong>{:+.3} s</strong></span><span class=\"tc\">{} → {}</span></label><div class=\"shift-body\">",
                    n + 1,
                    n + 1,
                    b.end - b.first,
                    b.delta_ms as f64 / 1000.,
                    rows::timecode(b.revised_start_ms, opts.revised_format),
                    rows::timecode(b.revised_end_ms, opts.revised_format)
                );
            }
            rows::row(&mut out, row, id, opts, extra);
            if blocks.peek().is_some_and(|(_, b)| b.end as usize == i + 1) {
                out.push_str("</div></div>");
                blocks.next();
            }
        }
        out.push_str("</section>");
        if !cue_discussion.is_empty() {
            let _ = write!(
                out,
                "<section class=\"detached\"><h2>Discussion on unchanged cues</h2>{cue_discussion}</section>"
            );
        }
        out.push_str(&extra.end_html);
    }
    out.push_str("</main></body></html>\n");
    out
}
