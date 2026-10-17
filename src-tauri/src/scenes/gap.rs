//! Immutable scene capture for explicit gap repair. Acceptance holds this scene
//! mutex through the document transaction: media selection -> scenes -> document.
use super::*;
use subtitle_core::Milliseconds;
use vsf_application::quality::gap::Scenes as GapScenes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GapKey {
    pub session: u64,
    pub attached: bool,
    generation: Option<u64>,
    ready: bool,
}
pub(crate) struct GapCapture {
    pub key: GapKey,
    pub data: GapScenes,
}
impl Job {
    fn gap_key(&self, session: u64, attached: bool) -> GapKey {
        GapKey {
            session,
            attached,
            generation: attached.then_some(self.generation),
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
    pub(crate) fn capture_gap(&self, session: u64, attached: bool) -> Result<GapCapture> {
        let job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        let key = job.gap_key(session, attached);
        let data = if !attached {
            GapScenes::NoMedia
        } else if !key.ready {
            GapScenes::Unavailable
        } else {
            if job.cuts.len() > 100_000 || job.cuts.windows(2).any(|p| p[0] >= p[1]) {
                return Err("Gap preview requires bounded ordered scene timestamps".into());
            }
            let mut cuts = Vec::with_capacity(job.cuts.len());
            for seconds in &job.cuts {
                let ms = (seconds * 1000.).round();
                if !ms.is_finite() || ms < 0. || ms > Milliseconds::MAX as f64 {
                    return Err("Invalid scene timestamp for gap preview".into());
                }
                cuts.push(Milliseconds::new(ms as u64)?);
            }
            // Different sub-millisecond cuts may round to the same domain point.
            cuts.dedup();
            GapScenes::Complete(cuts)
        };
        Ok(GapCapture { key, data })
    }
    pub(crate) fn with_gap_current<T>(
        &self,
        key: GapKey,
        operation: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let job = self.job.lock().map_err(|_| "Scene state unavailable")?;
        if job.gap_key(key.session, key.attached) != key {
            return Err("Scene data changed after gap preview; create a new proposal".into());
        }
        operation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_capture_rejects_partial_stale_and_replaced_data_and_rounds_once() {
        let scenes = Scenes::default();
        assert!(matches!(
            scenes.capture_gap(0, false).unwrap().data,
            GapScenes::NoMedia
        ));
        assert!(matches!(
            scenes.capture_gap(7, true).unwrap().data,
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
        let capture = scenes.capture_gap(7, true).unwrap();
        let GapScenes::Complete(cuts) = capture.data else {
            panic!("completed scene data absent")
        };
        assert_eq!(
            cuts.iter().map(|t| t.get()).collect::<Vec<_>>(),
            [1000, 1235]
        );
        assert!(scenes.with_gap_current(capture.key, || Ok(())).is_ok());
        scenes.job.lock().unwrap().running = true;
        assert!(scenes.with_gap_current(capture.key, || Ok(())).is_err());
        assert!(matches!(
            scenes.capture_gap(7, true).unwrap().data,
            GapScenes::Unavailable
        ));
        scenes.job.lock().unwrap().running = false;
        scenes.job.lock().unwrap().partial = true;
        assert!(matches!(
            scenes.capture_gap(7, true).unwrap().data,
            GapScenes::Unavailable
        ));
        scenes.invalidate();
        assert!(scenes.with_gap_current(capture.key, || Ok(())).is_err());
    }
}
