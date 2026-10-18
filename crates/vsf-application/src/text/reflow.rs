use super::{Projection, Row};
use crate::quality::{Profile, metrics};

pub(super) fn row(
    index: usize,
    source: &str,
    p: &Projection,
    profile: &Profile,
    join: bool,
) -> Row {
    let mut row = Row {
        cue_index: index,
        matches: vec![],
        after: None,
        skipped: None,
    };
    if join && !(if p.ass_document { &p.visible } else { source }).contains('\n') {
        return row;
    }
    let reason = p.ambiguous.or_else(|| {
        if !profile.rules.lines.enabled {
            Some("Line rules are disabled in the active quality profile.")
        } else if p.layout {
            Some("Positioned or animated text needs manual line-layout review.")
        } else if p.escapes && !p.ass_document {
            Some("ASS text escapes need manual reflow; source counting differs from rendering.")
        } else if p
            .visible
            .lines()
            .any(|l| l.trim_start().starts_with(['-', '–', '—', '>']))
        {
            Some("Dialogue or list markers need manual line-layout review.")
        } else if p.visible.contains("\n\n") || p.visible.contains('\t') {
            Some("Blank lines or tabs need manual spacing review.")
        } else {
            None
        }
    });
    if let Some(reason) = reason {
        row.skipped = Some(reason);
        return row;
    }
    // Only ASCII spaces/newlines are break opportunities. NBSP, punctuation,
    // words, combining characters and emoji sequences retain their exact bytes.
    let mut words = vec![];
    let mut start = 0;
    for (at, c) in p.visible.char_indices() {
        if c == ' ' || c == '\n' {
            if start < at {
                words.push(start..at);
            }
            start = at + 1;
        }
    }
    if start < p.visible.len() {
        words.push(start..p.visible.len());
    }
    if words.is_empty() {
        return row;
    }
    if words[0].start != 0 || words.last().expect("words").end != p.visible.len() {
        row.skipped = Some("Leading or trailing whitespace needs manual review.");
        return row;
    }
    let lengths: Vec<_> = words
        .iter()
        .map(|r| p.visible[r.clone()].encode_utf16().count())
        .collect();
    if words.len() == 1 {
        if lengths[0] > profile.rules.lines.maximum_characters as usize {
            row.skipped = Some("There is no safe word boundary within the profile line limit.");
        }
        return row;
    }
    let maximum = profile.rules.lines.maximum_characters as usize;
    let preferred = profile.rules.lines.single_line_preferred as usize;
    let max_lines = profile.rules.lines.maximum_lines as usize;
    let total = lengths.iter().sum::<usize>() + lengths.len() - 1;
    let mut breaks = vec![false; words.len() - 1];
    if lengths.iter().any(|&n| n > maximum) || (join && total > preferred.min(maximum)) {
        row.skipped =
            Some("Joining or splitting these words cannot satisfy the profile line limits.");
        return row;
    }
    if !join && total > preferred.min(maximum) {
        // Prefer a balanced two-line layout. For profiles allowing more lines,
        // fall back to bounded greedy wrapping without splitting any word.
        let mut left = 0;
        let mut best = None;
        for (i, &length) in lengths.iter().take(lengths.len() - 1).enumerate() {
            left += length + usize::from(i > 0);
            let right = total - left - 1;
            if left <= maximum && right <= maximum && max_lines >= 2 {
                let score = left.abs_diff(right);
                if best.is_none_or(|(_, old)| score < old) {
                    best = Some((i, score));
                }
            }
        }
        if let Some((i, _)) = best {
            breaks[i] = true;
        } else {
            let mut width = lengths[0];
            let mut lines = 1;
            for (i, &length) in lengths.iter().enumerate().skip(1) {
                if width + 1 + length > maximum {
                    breaks[i - 1] = true;
                    lines += 1;
                    width = length;
                } else {
                    width += 1 + length;
                }
            }
            if lines > max_lines {
                row.skipped =
                    Some("The text cannot fit the profile line count without splitting words.");
                return row;
            }
        }
    }
    let mut patches = vec![];
    for (i, pair) in words.windows(2).enumerate() {
        let range = p.source_range(pair[0].end..pair[1].start);
        let first_token = p
            .protected
            .partition_point(|token| token.end <= range.start);
        let plain_whitespace = p.ass_document
            && p.protected[first_token..]
                .iter()
                .take_while(|token| token.start < range.end)
                .filter(|token| token.start < range.end && token.end > range.start)
                .all(|token| {
                    token.start >= range.start
                        && token.end <= range.end
                        && matches!(&source[token.clone()], "\\N" | "\\n")
                });
        if !p.safe_range(source, range.clone()) && !plain_whitespace {
            row.skipped =
                Some("Whitespace crosses a formatting boundary; adjust the lines manually.");
            return row;
        }
        patches.push((
            range,
            if breaks[i] {
                if p.ass_document { "\\N" } else { "\n" }
            } else {
                " "
            },
        ));
    }
    let mut after = String::with_capacity(source.len());
    let mut at = 0;
    for (range, whitespace) in patches {
        after.push_str(&source[at..range.start]);
        after.push_str(whitespace);
        at = range.end;
    }
    after.push_str(&source[at..]);
    if after != source {
        // Check the actual stored source against the named counting policy.
        let valid = if p.ass_document {
            // Wrapping is an explicit display operation. Keep the named historical
            // quality counters unchanged; they describe source, including escapes.
            let projection = Projection::for_ass_document(&after);
            let lines: Vec<_> = projection.visible.split('\n').collect();
            projection.ambiguous.is_none()
                && lines.len() <= max_lines
                && lines
                    .iter()
                    .all(|line| line.encode_utf16().count() <= maximum)
        } else {
            metrics::text(&after, || Ok(())).is_ok_and(|m| {
                m.lines.len() <= max_lines && m.lines.iter().all(|&n| n as usize <= maximum)
            })
        };
        if valid {
            row.after = Some(after);
        } else {
            row.skipped = Some("The resulting source would still exceed the profile line limits.");
        }
    }
    row
}
