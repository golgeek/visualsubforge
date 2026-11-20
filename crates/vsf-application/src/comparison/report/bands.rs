use super::*;
use metrics::Rating;

#[derive(Default)]
pub(super) struct Distribution {
    // Quality ratings in Rating::ALL order, followed by unrated cues.
    counts: [[usize; 2]; 10],
}

impl Distribution {
    pub(super) fn new(changes: &[Change]) -> Self {
        let mut distribution = Self::default();
        for change in changes {
            for (side, cues) in [&change.original, &change.revised].into_iter().enumerate() {
                for cue in cues {
                    let row = cue
                        .duration_rating()
                        .map_or(Rating::ALL.len(), |r| r as usize);
                    distribution.counts[row][side] += 1;
                }
            }
        }
        distribution
    }
    pub(super) fn outer_delta(&self) -> i64 {
        [
            Rating::TooSlow,
            Rating::SlowAcceptable,
            Rating::FastAcceptable,
            Rating::TooFast,
        ]
        .iter()
        .map(|&r| {
            let [before, after] = self.counts[r as usize];
            after as i64 - before as i64
        })
        .sum()
    }
    pub(super) fn render(&self, out: &mut String) {
        let max = self
            .counts
            .iter()
            .flatten()
            .copied()
            .max()
            .unwrap_or(1)
            .max(1) as f64;
        let [before, after] = [0, 1].map(|side| self.counts.iter().map(|n| n[side]).sum::<usize>());
        let _ = write!(
            out,
            r#"<section class="card"><div class="card-head"><h2>Reading speed, before and after</h2><p>Cues per band, by cue duration.</p></div><div class="bands"><div class="bands-head"><span></span><span>Original · {before} cues</span><span>Revised · {after} cues</span></div>"#
        );
        for (i, [before, after]) in self.counts.iter().copied().enumerate() {
            let rating = Rating::ALL.get(i).copied();
            let colour = match rating {
                Some(Rating::TooSlow | Rating::TooFast) => "bad",
                Some(Rating::SlowAcceptable | Rating::FastAcceptable) => "ok",
                Some(Rating::ABitSlow | Rating::ABitFast) => "meh",
                Some(_) => "good",
                None if before + after == 0 => continue,
                None => "none",
            };
            let label = rating.map_or("Not rated", Rating::label);
            let _ = write!(
                out,
                r#"<div class="band"><span>{label}</span><span class="bar-l">{before}<i class="b orig" style="width:{:.1}%"></i></span><span class="bar-r"><i class="b rs-{colour}" style="width:{:.1}%"></i>{after}</span></div>"#,
                before as f64 * 100.0 / max,
                after as f64 * 100.0 / max
            );
        }
        out.push_str("</div></section>");
    }
}
