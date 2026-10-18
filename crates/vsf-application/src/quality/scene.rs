//! Scene inputs and endpoint policy, independent of media, filesystem and UI.
//! A completed visual analysis is heuristic, including a completed empty result.
mod repair;
use super::{CueMetrics, Error, Finding, Profile, Result, Rule, Severity};
pub use repair::{Proposal, Row, Skip, propose, propose_for_format};
use serde::{Deserialize, Serialize};
use subtitle_core::Milliseconds;

/// Adapters round actual timestamps to nearest ms (ties upward), then deduplicate.
#[derive(Clone, Debug)]
pub enum Scenes {
    NoMedia,
    Unavailable,
    Partial,
    Complete(Vec<Milliseconds>),
    Reduced(Vec<Milliseconds>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SceneCoverage {
    NotRequired,
    Checked,
    Reduced,
    NoMedia,
    Unavailable,
    Partial,
}
impl SceneCoverage {
    /// Complete relative to the supplied input, not proof that every cut exists.
    pub fn available(self) -> bool {
        matches!(self, Self::NotRequired | Self::Checked | Self::Reduced)
    }
}

/// Open intervals use signed bounds so a cut near zero still excludes time zero.
/// Strictly overlapping windows merge; touching windows retain their safe point.
pub(super) struct Windows {
    coverage: SceneCoverage,
    intervals: Vec<(i64, i64)>,
}
impl Windows {
    pub(super) fn new(
        profile: &Profile,
        scenes: &Scenes,
        checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Self> {
        Self::for_format(profile, scenes, subtitle_formats::Format::Srt, checkpoint)
    }
    /// Expanding open bounds to grid edges preserves exactly the permitted grid
    /// points. Merge expansions once so each later endpoint lookup stays O(log n).
    pub(super) fn for_format(
        profile: &Profile,
        scenes: &Scenes,
        format: subtitle_formats::Format,
        mut checkpoint: impl FnMut() -> Result<()>,
    ) -> Result<Self> {
        checkpoint()?;
        let before = i64::from(profile.timing.scene_before_ms);
        let after = i64::from(profile.timing.scene_after_ms);
        let coverage = if before == 0 && after == 0 {
            SceneCoverage::NotRequired
        } else {
            match scenes {
                Scenes::NoMedia => SceneCoverage::NoMedia,
                Scenes::Unavailable => SceneCoverage::Unavailable,
                Scenes::Partial => SceneCoverage::Partial,
                Scenes::Complete(_) => SceneCoverage::Checked,
                Scenes::Reduced(_) => SceneCoverage::Reduced,
            }
        };
        let mut result = Self {
            coverage,
            intervals: Vec::new(),
        };
        if let Scenes::Complete(cuts) | Scenes::Reduced(cuts) = scenes {
            if cuts.len() > 100_000 {
                return Err(Error::Limit(
                    "Scene checks support at most 100000 cuts".into(),
                ));
            }
            let mut previous = None;
            for cut in cuts {
                checkpoint()?;
                if previous.is_some_and(|p| p >= *cut) {
                    return Err(Error::Invalid(
                        "Scene times must be strictly ordered and unique".into(),
                    ));
                }
                previous = Some(*cut);
                if coverage == SceneCoverage::NotRequired {
                    continue;
                }
                let step = format.time_step_ms() as i64;
                let left = (cut.get() as i64 - before).div_euclid(step) * step;
                let right = -(-(cut.get() as i64 + after)).div_euclid(step) * step;
                if let Some(last) = result.intervals.last_mut()
                    && left < last.1
                {
                    last.1 = right;
                } else {
                    result.intervals.push((left, right));
                }
            }
        }
        Ok(result)
    }
    pub(super) fn coverage(&self) -> SceneCoverage {
        self.coverage
    }
    pub(super) fn containing(&self, time: u64) -> Option<(i64, i64)> {
        let time = time as i64;
        let at = self.intervals.partition_point(|&(_, right)| right <= time);
        self.intervals
            .get(at)
            .copied()
            .filter(|&(left, _)| left < time)
    }
    pub(super) fn check(&self, row: &CueMetrics, out: &mut Vec<Finding>) {
        for (time, rule, endpoint) in [
            (row.start_ms, Rule::SceneStart, "Start"),
            (row.end_ms, Rule::SceneEnd, "End"),
        ] {
            if let Some((left, right)) = self.containing(time as u64) {
                out.push(Finding {
                    rule, cue_id: row.cue_id.clone(), cue_index: row.cue_index,
                    related_cue_id: None, severity: Severity::Violation,
                    measured: Some(time),
                    expected: format!("Endpoint outside ({left}, {right}) ms; outer edges allowed"),
                    explanation: format!("{endpoint} is strictly inside a configured scene buffer. Completed visual detection is heuristic; crossing a cut alone is allowed."),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
