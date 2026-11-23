use super::*;
use crate::quality::{CueMetrics, Profile};

pub(super) fn timecode(time: u64) -> String {
    subtitle_formats::srt::format_time(Milliseconds::new(time).expect("checked subtitle endpoint"))
}

pub(super) fn family_name(rule: Rule) -> &'static str {
    match rule {
        Rule::CpsHigh | Rule::CpsLow => "CPS",
        Rule::RsHigh | Rule::RsLow => "reading-speed",
        Rule::SpeedUnavailable => "unavailable reading-speed",
        Rule::DurationLong | Rule::DurationShort => "duration",
        Rule::Gap => "minimum-gap",
        Rule::Overlap => "overlap",
        Rule::SceneStart | Rule::SceneEnd => "shot-change",
        _ => "quality",
    }
}
impl Budget {
    pub(super) fn description(&self) -> String {
        let mut line = match (self.shorten_by, self.max_counted) {
            (Some(count), Some(maximum)) => format!(
                "Shorten by {count} counted {} (maximum {maximum}).",
                if count == 1 {
                    "character"
                } else {
                    "characters"
                }
            ),
            _ => "A text budget is unavailable within the 500 ms reading allowance.".into(),
        };
        if self.estimate {
            line.push_str(" Estimate with overlap-aware reading.");
        }
        if self.at_partial {
            line.push_str(&format!(
                " At the unticked timing {} → {}.",
                timecode(self.at.start_ms),
                timecode(self.at.end_ms)
            ));
        }
        line
    }
}
impl Manual {
    pub(super) fn new(index: usize, rules: Vec<Rule>, budget: Option<Budget>) -> Self {
        Self {
            index: index as u32,
            lead: "No allowed timing satisfies the enabled rules.".into(),
            facts: Vec::new(),
            blocking_cue: None,
            budget_text: budget.as_ref().map(Budget::description),
            rules,
            budget,
        }
    }
    pub fn unticked(index: usize) -> Self {
        let mut row = Self::new(index, Vec::new(), None);
        row.lead = "The change for this cue is unticked.".into();
        row
    }
    pub(super) fn explain(&mut self, context: &ReasonContext<'_>) {
        if let Some(cause) = context.blocked {
            self.lead = cause.into();
            return;
        }
        if let Some(root) = context.withheld {
            let number = root + 1;
            self.lead =
                format!("Depends on a change to cue {number} that would break a protected rule.");
            self.blocking_cue = Some(number);
            return;
        }
        let Some((index, next_start, in_scope)) = context.next else {
            return;
        };
        let number = index + 1;
        let time = timecode(next_start);
        let profile = context.profile;
        let cap = next_start.saturating_sub(profile.timing.cue_gap_ms.into());
        let at = context.at;
        let gap = self
            .rules
            .iter()
            .any(|r| matches!(r, Rule::Gap | Rule::Overlap));
        if gap && cap < at.end_ms {
            let spacing = if self.rules.contains(&Rule::Overlap) {
                "Overlap"
            } else {
                "Gap"
            };
            let duration = cap.saturating_sub(context.metric.reading_start_ms as u64);
            let (cps, rs, _) =
                metrics::rates(context.metric.characters, duration, context.metric.empty);
            let mut rates = Vec::new();
            for (label, old, new, limit, exclusive) in [
                (
                    "CPS",
                    &context.metric.cps,
                    cps,
                    profile.rules.speed.maximum_cps,
                    false,
                ),
                (
                    "RS",
                    &context.metric.rs,
                    rs,
                    profile.rules.speed.maximum_rs_exclusive,
                    true,
                ),
            ] {
                if profile.rules.speed.enabled
                    && let (Some(before), Some(after), Some(limit)) =
                        (old.value(), new.value(), limit)
                    && after > before
                    && (after > f64::from(limit) || exclusive && after == f64::from(limit))
                {
                    rates.push(format!("{label} {before:.1} → {after:.1} (limit {limit})"));
                }
            }
            let minimum = profile.rules.duration.minimum_ms;
            if !rates.is_empty() {
                self.lead = format!("{spacing} not fixed: this cue would become too fast.");
                self.facts.push(format!("{}.", rates.join(", ")));
                let delay = at.end_ms - cap;
                self.facts.push(format!("Cue {number} starts at {time}. Delaying it needs {delay} ms of start moves (now {}).",context.tolerance));
            } else if profile.rules.duration.enabled
                && cap.saturating_sub(at.start_ms) < minimum.into()
            {
                self.lead = format!(
                    "{spacing} not fixed: this cue would fall below the {minimum} ms minimum."
                );
                self.facts.push(format!(
                    "The latest end before cue {number} is {}.",
                    timecode(cap)
                ));
            } else if let Some((left, right)) = context.windows.containing(cap) {
                self.lead =
                    format!("{spacing} not fixed: the end would land inside a shot-change window.");
                self.facts.push(format!(
                    "The window runs from {} to {}.",
                    timecode(left.max(0) as u64),
                    timecode((right as u64).min(Milliseconds::MAX))
                ));
            } else if !in_scope {
                self.lead = format!("Cue {number} is outside the scope and holds this time.");
            } else {
                self.lead = "No allowed timing satisfies the enabled rules.".into();
                self.facts.push(format!("Cue {number} starts at {time}."));
            }
        } else if self.rules.contains(&Rule::SceneStart)
            && !context.families.gaps
            && let Some((_, edge)) = context.windows.containing(at.start_ms)
            && edge as u64 >= next_start
        {
            self.lead = "Not fixed: it would make gaps and overlaps worse.".into();
            self.facts.push(format!(
                "The inward edge is at {}, past the space before cue {number}.",
                timecode((edge as u64).min(Milliseconds::MAX))
            ));
        } else if !in_scope {
            self.lead = format!("Cue {number} is outside the scope and holds this time.");
        } else if self.budget.is_some() {
            self.lead = format!("Too fast: cue {number} starts at {time}.");
        } else if self.rules.contains(&Rule::DurationShort) {
            self.lead = format!("Too short: cue {number} starts at {time}.");
            self.facts.push(format!(
                "The minimum duration is {} ms.",
                profile.rules.duration.minimum_ms
            ));
        } else {
            return;
        }
        self.blocking_cue = Some(number);
    }
}
pub(super) struct ReasonContext<'a> {
    pub families: Families,
    pub blocked: Option<&'a str>,
    pub withheld: Option<u32>,
    pub next: Option<(u32, u64, bool)>,
    pub at: Endpoints,
    pub metric: &'a CueMetrics,
    pub profile: &'a Profile,
    pub windows: &'a Windows,
    pub tolerance: u32,
}
