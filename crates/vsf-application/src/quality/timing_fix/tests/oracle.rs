//! Check the resulting document directly. Neither the planner's severity
//! vector nor its selection validator defines the expected result here.
use super::*;
use sha2::{Digest, Sha256};

struct Generator(u64);
impl Generator {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }
    fn pick<T: Copy>(&mut self, values: &[T]) -> T {
        values[self.below(values.len() as u64) as usize]
    }
}

pub(super) struct Trial {
    pub(super) document: Document,
    pub(super) profile: Profile,
    pub(super) scenes: Scenes,
    pub(super) format: Format,
    pub(super) request: Request,
}

impl Trial {
    fn generate(random: &mut Generator) -> Self {
        let format = random.pick(&[Format::Srt, Format::WebVtt, Format::Ass]);
        let step = format.time_step_ms();
        let mut rows = Vec::new();
        let mut cuts = Vec::new();
        let mut frontier = 0i64;
        let mut previous = 0;
        for _ in 0..4 + random.below(57) {
            let gap = random.pick(&[-600, -200, -40, 0, 20, 40, 79, 80, 150, 400, 1500]);
            let start = ((frontier + gap).max(0) as u64 / step * step).max(previous + step);
            let end = start + random.pick(&[200, 400, 600, 800, 1000, 1500, 2200, 3500, 7200]);
            rows.push((
                start,
                end,
                random.pick(&[
                    "Yes.",
                    "Is this enough time for the whole sentence?",
                    "a😀\nb",
                    "<i>Somewhere far away...</i>",
                    "We should leave before the rain starts.",
                ]),
            ));
            for point in [start, end] {
                if random.below(4) == 0 {
                    cuts.push(ms((point + random.below(400)).saturating_sub(200)));
                }
            }
            frontier = frontier.max(end as i64);
            previous = start;
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut profile = Profile::default().custom("oracle".into(), "Oracle".into());
        profile.speed_basis = random.pick(&[SpeedBasis::OrdinaryV1, SpeedBasis::VssOverlapV1]);
        (
            profile.timing.scene_before_ms,
            profile.timing.scene_after_ms,
        ) = random.pick(&[(130, 130), (0, 0), (60, 120), (250, 40)]);
        let mask = random.below(8);
        let request = Request {
            scope: (random.below(4) == 0).then(|| {
                (1..=rows.len())
                    .filter(|_| random.below(2) == 0)
                    .map(|n| CueId(n as u64))
                    .collect()
            }),
            families: Families {
                gaps: mask & 1 != 0,
                speed: mask & 2 != 0,
                shots: mask & 4 != 0,
            },
            options: Options {
                priority: random.pick(&[Priority::ReadingSpeed, Priority::Spacing]),
                start_tolerance_ms: random.pick(&[0, 100, 300, 1000]),
                scene_edges: random.pick(&[SceneEdges::Nearest, SceneEdges::Inward]),
            },
        };
        Self {
            document: doc(&rows),
            profile,
            scenes: if random.below(6) == 0 {
                Scenes::NoMedia
            } else {
                Scenes::Complete(cuts)
            },
            format,
            request,
        }
    }
    pub(super) fn plan(&self) -> Proposal {
        propose(
            self.document.clone(),
            Evaluation::from(&self.profile).with_format(self.format),
            &self.scenes,
            self.request.clone(),
            || Ok(()),
        )
        .unwrap()
    }
    fn windows(&self) -> Vec<(i64, i64)> {
        let Scenes::Complete(cuts) = &self.scenes else {
            return vec![];
        };
        let step = self.format.time_step_ms() as i64;
        let before = i64::from(self.profile.timing.scene_before_ms);
        let after = i64::from(self.profile.timing.scene_after_ms);
        let mut intervals: Vec<(i64, i64)> = vec![];
        if before + after == 0 {
            return intervals;
        }
        for cut in cuts {
            let left = (cut.get() as i64 - before).div_euclid(step) * step;
            let right = -(-(cut.get() as i64 + after)).div_euclid(step) * step;
            if let Some(last) = intervals.last_mut()
                && left < last.1
            {
                last.1 = last.1.max(right);
            } else {
                intervals.push((left, right));
            }
        }
        intervals
    }
    pub(super) fn check(&self, proposal: &Proposal, selected: &[u32], candidate: &Document) {
        let old = self.document.cues();
        let new = candidate.cues();
        let order = |cues: &[Cue]| {
            let mut indices = (0..cues.len()).collect::<Vec<_>>();
            indices.sort_by_key(|&i| (cues[i].start(), i));
            indices
        };
        assert_eq!(order(old), order(new));
        let windows = self.windows();
        let inside = |time: u64| {
            windows
                .iter()
                .find(|&&(left, right)| left < (time as i64) && (time as i64) < right)
        };
        let overlap = |a: &Cue, b: &Cue| {
            a.end()
                .min(b.end())
                .get()
                .saturating_sub(a.start().max(b.start()).get())
        };
        for (index, (before, after)) in old.iter().zip(new).enumerate() {
            assert!(after.start() < after.end());
            assert_eq!(after.id(), before.id());
            for (from, to) in [(before.start(), after.start()), (before.end(), after.end())] {
                assert_eq!(to.get() % self.format.time_step_ms(), 0);
                if from != to {
                    assert!(inside(to.get()).is_none());
                    assert!(
                        self.request
                            .scope
                            .as_ref()
                            .is_none_or(|scope| scope.contains(&before.id()))
                    );
                }
            }
            if before.start().get().abs_diff(after.start().get())
                > u64::from(self.request.options.start_tolerance_ms)
            {
                assert!(
                    self.request.families.shots
                        && inside(before.start().get())
                            .is_some_and(|&(_, right)| right == after.start().get() as i64)
                );
            }
            let duration = |cue: &Cue| cue.end().get() - cue.start().get();
            let bounds = &self.profile.rules.duration;
            if bounds.enabled {
                assert!(
                    duration(after).saturating_sub(bounds.maximum_ms.into())
                        <= duration(before).saturating_sub(bounds.maximum_ms.into())
                );
                assert!(
                    u64::from(bounds.minimum_ms).saturating_sub(duration(after))
                        <= u64::from(bounds.minimum_ms).saturating_sub(duration(before))
                );
            }
            for other in index + 1..old.len() {
                assert!(overlap(after, &new[other]) <= overlap(before, &old[other]));
            }
        }
        let report = evaluate_with_scenes(
            candidate,
            Evaluation::from(&self.profile).with_format(self.format),
            &self.scenes,
            |_, _| Ok(()),
        )
        .unwrap();
        for f in &report.findings {
            let previous = proposal.before.findings.iter().find(|old| {
                old.rule == f.rule
                    && old.cue_id == f.cue_id
                    && old.related_cue_id == f.related_cue_id
            });
            let worse = match f.rule {
                Rule::CpsHigh | Rule::RsHigh | Rule::DurationLong => {
                    previous.is_none_or(|old| f.measured > old.measured)
                }
                Rule::CpsLow | Rule::RsLow | Rule::DurationShort | Rule::Overlap | Rule::Gap => {
                    previous.is_none_or(|old| f.measured < old.measured)
                }
                Rule::SceneStart | Rule::SceneEnd | Rule::SpeedUnavailable => {
                    previous.is_none_or(|old| f.measured != old.measured)
                }
                _ => false,
            };
            if worse {
                assert!(
                    matches!(f.rule, Rule::CpsHigh | Rule::RsHigh)
                        && self.request.options.priority == Priority::Spacing
                        && self.request.families.speed
                        && (self.request.families.gaps || self.request.families.shots)
                        && proposal
                            .units
                            .iter()
                            .any(|unit| selected.contains(&unit.index)
                                && unit.trade_off
                                && unit.effects.iter().any(|effect| effect.index == f.cue_index
                                    && effect.rule == f.rule
                                    && effect.after >= f.measured))
                );
            }
        }
    }
}
#[test]
fn independent_selection_oracle() {
    let cases = std::env::var("VSF_TIMING_ORACLE_CASES")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(20);
    let mut random = Generator(0x035f_2026);
    let mut histogram = [0usize; 4];
    let mut rejected = 0;
    let mut with_partial = 0;
    let mut digest = Sha256::new();
    for case in 0..cases {
        let trial = Trial::generate(&mut random);
        let proposal = trial.plan();
        histogram[proposal.evaluations] += 1;
        let snapshot =
            |p: &Proposal| serde_json::to_vec(&(&p.units, &p.manual, &p.after.findings)).unwrap();
        let serialized = snapshot(&proposal);
        assert_eq!(serialized, snapshot(&trial.plan()), "case {case}");
        digest.update(serialized);
        let mut toggles = Vec::new();
        for action in 0..5 {
            if action > 0 && !proposal.units.is_empty() {
                toggles.push(
                    random.pick(
                        &proposal
                            .units
                            .iter()
                            .map(|unit| unit.index)
                            .collect::<Vec<_>>(),
                    ),
                );
            }
            let (selected, plan) = proposal
                .review_selection(&toggles, action == 4, || Ok(()))
                .unwrap();
            if plan.error.is_some() {
                assert_ne!(action, 0, "invalid default in case {case}");
                rejected += 1;
                if std::env::var_os("VSF_TIMING_ORACLE_CONFLICTS").is_some() {
                    eprintln!(
                        "conflict {}",
                        serde_json::json!({"case":case,"actions":toggles,"omitTrades":action==4,"conflicts":plan.selection_conflicts,"partialUnits":proposal.units.iter().filter(|u|u.partial && selected.contains(&u.index)).map(|u|u.index).collect::<Vec<_>>(),"message":plan.error})
                    );
                }
                assert!(
                    !plan.selection_conflicts.is_empty(),
                    "unattributed conflict in case {case}"
                );
                if proposal
                    .units
                    .iter()
                    .any(|u| u.partial && selected.contains(&u.index))
                {
                    with_partial += 1;
                }
                if rejected <= 3 {
                    eprintln!("case {case}, actions {toggles:?}, rejected {selected:?}");
                }
            } else {
                trial.check(&proposal, &selected, &plan.candidate);
            }
        }
    }
    eprintln!(
        "cases={cases} evaluations={histogram:?} rejected={rejected} with_partial={with_partial} SHA256={:x}",
        digest.finalize()
    );
}
