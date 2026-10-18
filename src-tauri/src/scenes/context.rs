//! Immutable scene capture for quality checks and timing repairs. Acceptance holds this scene
//! mutex through the document transaction: media selection -> scenes -> document.
use super::*;
use subtitle_core::Milliseconds;
use vsf_application::quality::gap::Scenes as GapScenes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SceneKey {
    pub session: u64,
    pub attached: bool,
    generation: Option<u64>,
    ready: bool,
    partial: bool,
    version: Option<u32>,
}
pub(crate) struct SceneCapture {
    pub key: SceneKey,
    pub data: GapScenes,
}
impl Job {
    fn quality_key(&self, session: u64, attached: bool) -> SceneKey {
        SceneKey {
            session,
            attached,
            generation: attached.then_some(self.generation),
            partial: attached && self.session == session && (self.partial || self.running),
            version: attached.then_some(self.analysis_version),
            ready: attached
                && self.session == session
                && !self.running
                && !self.partial
                && self.error.is_none()
                && self.mode == Some(Mode::Visual)
                && self.identity.is_some(),
        }
    }
}
impl Scenes {
    #[cfg(test)]
    pub(crate) fn test_completed(&self, session: u64, cuts: Vec<f64>, reduced: bool) {
        let saved = serde_json::from_value(serde_json::json!({
            "version": if reduced { 4 } else { 3 }, "mode": "visual", "cuts": cuts,
            "identity": {"size": 1, "modified": "fixture", "sampleHash": "fixture"}
        }))
        .unwrap();
        self.restore(session, saved).unwrap();
    }
    pub(crate) fn capture_quality(&self, session: u64, attached: bool) -> Result<SceneCapture> {
        let job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        let key = job.quality_key(session, attached);
        let data = if !attached {
            GapScenes::NoMedia
        } else if key.partial {
            GapScenes::Partial
        } else if !key.ready {
            GapScenes::Unavailable
        } else {
            if job.cuts.len() > 100_000 || job.cuts.windows(2).any(|p| p[0] >= p[1]) {
                return Err("Timing preview requires bounded ordered scene timestamps".into());
            }
            let mut cuts = Vec::with_capacity(job.cuts.len());
            for seconds in &job.cuts {
                let ms = (seconds * 1000.).round();
                if !ms.is_finite() || ms < 0. || ms > Milliseconds::MAX as f64 {
                    return Err("Invalid scene timestamp for timing preview".into());
                }
                cuts.push(Milliseconds::new(ms as u64)?);
            }
            // Different sub-millisecond cuts may round to the same domain point.
            cuts.dedup();
            if matches!(job.analysis_version, 2 | 4) {
                GapScenes::Reduced(cuts)
            } else {
                GapScenes::Complete(cuts)
            }
        };
        Ok(SceneCapture { key, data })
    }
    pub(crate) fn with_quality_current<T>(
        &self,
        key: SceneKey,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        if job.quality_key(key.session, key.attached) != key {
            return Err("Scene data changed after timing preview; create a new proposal".into());
        }
        operation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_preserves_actual_fractional_timestamps_and_restored_reduced_provenance() {
        let scenes = Scenes::default();
        scenes.test_completed(
            7,
            vec![0., 1001. / 24000., 1001. / 30000. * 2., 1.2345, 1.2346],
            true,
        );
        let capture = scenes.capture_quality(7, true).unwrap();
        let GapScenes::Reduced(cuts) = capture.data else {
            panic!("reduced coverage lost")
        };
        assert_eq!(
            cuts.iter().map(|t| t.get()).collect::<Vec<_>>(),
            [0, 42, 67, 1235]
        );
        scenes.test_completed(7, vec![], false);
        assert!(scenes.with_quality_current(capture.key, || Ok(())).is_err());
        assert!(
            matches!(scenes.capture_quality(7,true).unwrap().data,GapScenes::Complete(c) if c.is_empty())
        );
        assert!(matches!(
            scenes.capture_quality(8, true).unwrap().data,
            GapScenes::Unavailable
        ));
    }
    #[test]
    fn scene_capture_rejects_partial_stale_and_replaced_data_and_rounds_once() {
        let scenes = Scenes::default();
        assert!(matches!(
            scenes.capture_quality(0, false).unwrap().data,
            GapScenes::NoMedia
        ));
        assert!(matches!(
            scenes.capture_quality(7, true).unwrap().data,
            GapScenes::Unavailable
        ));
        {
            let mut job = scenes.job.lock().unwrap();
            job.session = 7;
            job.mode = Some(Mode::Visual);
            job.identity = Some(
                serde_json::from_value(serde_json::json!({
                    "size": 1, "modified": "fixture", "sampleHash": "fixture"
                }))
                .unwrap(),
            );
            job.cuts = vec![1.0001, 1.0002, 1.2346];
        }
        let capture = scenes.capture_quality(7, true).unwrap();
        let GapScenes::Complete(cuts) = capture.data else {
            panic!("completed scene data absent")
        };
        assert_eq!(
            cuts.iter().map(|t| t.get()).collect::<Vec<_>>(),
            [1000, 1235]
        );
        assert!(scenes.with_quality_current(capture.key, || Ok(())).is_ok());
        scenes.job.lock().unwrap().running = true;
        assert!(scenes.with_quality_current(capture.key, || Ok(())).is_err());
        assert!(matches!(
            scenes.capture_quality(7, true).unwrap().data,
            GapScenes::Partial
        ));
        scenes.job.lock().unwrap().running = false;
        scenes.job.lock().unwrap().partial = true;
        assert!(matches!(
            scenes.capture_quality(7, true).unwrap().data,
            GapScenes::Partial
        ));
        scenes.invalidate();
        assert!(scenes.with_quality_current(capture.key, || Ok(())).is_err());
    }
}
