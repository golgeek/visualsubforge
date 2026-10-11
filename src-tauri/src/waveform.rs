//! Cancellable analysis/cache. Completed results are accepted only for the current
//! analysis generation and media session. IPC contains bounded visible peaks.
use crate::analysis_tools::tool;
use crate::analysis_worker::Worker;
use media_analysis::{Cancellation, Tools, Waveform};
use serde::Serialize;
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::Manager;

type Result<T> = std::result::Result<T, String>;
#[derive(Clone)]
pub struct Waveforms {
    job: Arc<Mutex<Job>>,
    worker: Arc<Mutex<Worker>>,
}
impl Default for Waveforms {
    fn default() -> Self {
        Self {
            job: Arc::default(),
            worker: Arc::new(Mutex::new(Worker::new("waveform-analysis"))),
        }
    }
}
#[derive(Default)]
struct Job {
    generation: u64,
    session: u64,
    cancellation: Cancellation,
    running: bool,
    error: Option<String>,
    data: Option<Arc<Pyramid>>,
}
impl Job {
    fn finish(
        &mut self,
        generation: u64,
        session: u64,
        current_session: u64,
        result: Result<Pyramid>,
    ) -> bool {
        if self.generation != generation
            || self.session != session
            || self.cancellation.is_cancelled()
            || session != current_session
        {
            return false;
        }
        self.running = false;
        match result {
            Ok(data) => self.data = Some(Arc::new(data)),
            Err(error) => self.error = Some(error),
        }
        true
    }
}
struct Pyramid {
    levels: Vec<Vec<[f32; 2]>>,
    step: f64,
    duration: f64,
    origin: f64,
    stream: u32,
}
impl Pyramid {
    fn new(wave: Waveform) -> Self {
        let mut levels = vec![
            wave.peaks
                .into_iter()
                .map(|p| [p.min, p.max])
                .collect::<Vec<_>>(),
        ];
        while levels.last().is_some_and(|level| level.len() > 1) {
            let next = levels
                .last()
                .expect("base level exists")
                .chunks(4)
                .map(|chunk| {
                    chunk
                        .iter()
                        .fold([f32::INFINITY, f32::NEG_INFINITY], |a, p| {
                            [a[0].min(p[0]), a[1].max(p[1])]
                        })
                })
                .collect();
            levels.push(next);
        }
        Self {
            levels,
            step: f64::from(wave.samples_per_peak) / f64::from(wave.sample_rate),
            duration: wave.sample_count as f64 / f64::from(wave.sample_rate),
            origin: wave.container_origin_seconds,
            stream: wave.audio_stream_index,
        }
    }
    fn window(&self, start: f64, end: f64, pixels: usize) -> (f64, f64, Vec<[f32; 2]>) {
        let mut level = 0;
        let mut step = self.step;
        while level + 1 < self.levels.len() && (end - start) / step > pixels as f64 {
            level += 1;
            step *= 4.0;
        }
        let peaks = &self.levels[level];
        let first = (start / step).floor().max(0.0) as usize;
        let last = ((end / step).ceil().max(0.0) as usize).min(peaks.len());
        (
            first as f64 * step,
            step,
            if first < last {
                peaks[first..last].to_vec()
            } else {
                Vec::new()
            },
        )
    }
}
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WaveformView {
    #[serde(with = "crate::ipc_number")]
    #[specta(type = crate::ipc_number::SafeU64)]
    generation: u64,
    running: bool,
    error: Option<String>,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    duration_seconds: f64,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    container_origin_seconds: f64,
    audio_stream_index: Option<u32>,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    start_seconds: f64,
    #[serde(with = "crate::ipc_number::finite")]
    #[specta(type = crate::ipc_number::finite::FiniteF64)]
    seconds_per_peak: f64,
    #[serde(serialize_with = "crate::ipc_number::finite::serialize_peaks")]
    #[specta(type = Vec<[crate::ipc_number::finite::FiniteF64; 2]>)]
    peaks: Vec<[f32; 2]>,
}
impl Waveforms {
    /// Called on a blocking worker before process exit, so FFmpeg children are
    /// cancelled and reaped before the app/runtime is destroyed.
    pub fn shutdown(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            self.invalidate();
            worker.shutdown();
        }
    }

    pub fn cancel(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            worker.cancel();
            self.invalidate();
        }
    }
    fn invalidate(&self) {
        if let Ok(mut job) = self.job.lock() {
            job.cancellation.cancel();
            job.generation += 1;
            job.running = false;
            job.data = None;
            job.error = None;
        }
    }
}
#[tauri::command]
#[specta::specta]
pub async fn waveform_start(
    app: tauri::AppHandle,
    path: String,
    audio_ordinal: u32,
    session: crate::ipc_number::SafeU64,
) -> Result<crate::ipc_number::SafeU64> {
    let session = session.get();
    if app.state::<crate::media::Media>().current_session() != session {
        return Err("Media changed before waveform analysis started".into());
    }
    let state = app.state::<Waveforms>().inner().clone();
    let mut worker = state
        .worker
        .lock()
        .map_err(|_| "Waveform worker unavailable")?;
    if worker.is_closing() {
        return Err("Application is shutting down".into());
    }
    if app.state::<crate::media::Media>().current_session() != session {
        return Err("Media changed before waveform analysis started".into());
    }
    let (generation, cancellation) = {
        let mut job = state.job.lock().map_err(|_| "Waveform state unavailable")?;
        job.cancellation.cancel();
        job.generation += 1;
        job.session = session;
        job.cancellation = Cancellation::default();
        job.running = true;
        job.data = None;
        job.error = None;
        (job.generation, job.cancellation.clone())
    };
    let job_state = state.clone();
    worker
        .replace(cancellation.clone(), move || {
            let result = media_analysis::analyze(
                &Tools {
                    ffmpeg: tool("ffmpeg"),
                    ffprobe: tool("ffprobe"),
                },
                Path::new(&path),
                audio_ordinal,
                &cancellation,
                Duration::from_secs(600),
            )
            .map(Pyramid::new)
            .map_err(|e| e.to_string());
            if let Ok(mut job) = job_state.job.lock() {
                job.finish(
                    generation,
                    session,
                    app.state::<crate::media::Media>().current_session(),
                    result,
                );
            }
        })
        .inspect_err(|error| {
            if let Ok(mut job) = state.job.lock() {
                job.running = false;
                job.error = Some(error.clone());
            }
        })?;
    Ok(crate::ipc_number::SafeU64::try_from(generation)?)
}
#[tauri::command]
#[specta::specta]
pub async fn waveform_view(
    state: tauri::State<'_, Waveforms>,
    generation: crate::ipc_number::SafeU64,
    start: crate::ipc_number::finite::FiniteF64,
    end: crate::ipc_number::finite::FiniteF64,
    pixels: u32,
) -> Result<WaveformView> {
    let start = start.get();
    let end = end.get();
    let generation = generation.get();
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.0
        || end <= start
        || end > 360_000.0
        || !(1..=4096).contains(&pixels)
    {
        return Err("Invalid waveform viewport".into());
    }
    // The viewport check bounds this conversion on all supported targets.
    let pixels = pixels as usize;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (running, error, data) = {
            let job = state.job.lock().map_err(|_| "Waveform state unavailable")?;
            if job.generation != generation {
                return Err("Waveform was replaced".into());
            }
            (job.running, job.error.clone(), job.data.clone())
        };
        let mut view = WaveformView {
            generation,
            running,
            error,
            duration_seconds: 0.0,
            container_origin_seconds: 0.0,
            audio_stream_index: None,
            start_seconds: start,
            seconds_per_peak: 0.01,
            peaks: Vec::new(),
        };
        if let Some(data) = data {
            let (start, step, peaks) = data.window(start, end, pixels);
            view.start_seconds = start;
            view.seconds_per_peak = step;
            view.peaks = peaks;
            view.duration_seconds = data.duration;
            view.container_origin_seconds = data.origin;
            view.audio_stream_index = Some(data.stream);
        }
        Ok(view)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_aggregation_keeps_extrema_and_offset_mapping() {
        let peaks = (0..10001)
            .map(|i| media_analysis::Peak {
                min: if i == 5432 { -0.95 } else { -0.1 },
                max: if i == 5433 { 0.9 } else { 0.1 },
            })
            .collect();
        let pyramid = Pyramid::new(Waveform {
            container_origin_seconds: 5.0,
            audio_stream_index: 2,
            sample_rate: 8000,
            samples_per_peak: 80,
            sample_count: 800080,
            peaks,
        });
        let (start, step, peaks) = pyramid.window(50.0, 70.0, 100);
        assert!(peaks.len() <= 101);
        assert!(start <= 50.0);
        assert!(start + step > 50.0);
        assert_eq!(peaks.iter().map(|p| p[0]).reduce(f32::min), Some(-0.95));
        assert_eq!(peaks.iter().map(|p| p[1]).reduce(f32::max), Some(0.9));
        assert_eq!(pyramid.origin, 5.0);
        assert_eq!(pyramid.stream, 2);
        assert!(pyramid.window(200.0, 300.0, 100).2.is_empty());
    }
    #[test]
    fn cancellation_invalidates_generation_and_retained_data() {
        let state = Waveforms::default();
        let token = state.job.lock().unwrap().cancellation.clone();
        let (started, started_rx) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let (finished, finished_rx) = std::sync::mpsc::channel();
        let result_state = state.job.clone();
        state
            .worker
            .lock()
            .unwrap()
            .replace(token.clone(), move || {
                started.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let accepted =
                    result_state
                        .lock()
                        .unwrap()
                        .finish(0, 0, 0, Err("late failure".into()));
                finished.send(accepted).unwrap();
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        state.cancel();
        release.send(()).unwrap();
        assert!(!finished_rx.recv_timeout(Duration::from_secs(5)).unwrap());
        assert!(token.is_cancelled());
        {
            let job = state.job.lock().unwrap();
            assert_eq!(job.generation, 1);
            assert!(!job.running);
            assert!(job.data.is_none() && job.error.is_none());
        }
        state.shutdown();
    }
    #[test]
    fn late_result_cannot_replace_new_analysis_or_media_session() {
        let mut job = Job {
            generation: 2,
            session: 7,
            running: true,
            ..Job::default()
        };
        assert!(!job.finish(1, 7, 7, Err("old analysis".into())));
        assert!(!job.finish(2, 7, 8, Err("old media".into())));
        assert!(job.error.is_none());
        assert!(job.running);
        assert!(job.finish(2, 7, 7, Err("current failure".into())));
        assert_eq!(job.error.as_deref(), Some("current failure"));
        assert!(!job.running);
        job.cancellation.cancel();
        assert!(!job.finish(2, 7, 7, Err("cancelled result".into())));
    }
    #[test]
    fn shutdown_reaps_workers_before_returning() {
        let state = Waveforms::default();
        let token = state.job.lock().unwrap().cancellation.clone();
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let other = done.clone();
        let (started, started_rx) = std::sync::mpsc::channel();
        state
            .worker
            .lock()
            .unwrap()
            .replace(token.clone(), move || {
                started.send(()).unwrap();
                while !token.is_cancelled() {
                    std::thread::yield_now();
                }
                other.store(true, std::sync::atomic::Ordering::Release);
            })
            .unwrap();
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        state.shutdown();
        assert!(done.load(std::sync::atomic::Ordering::Acquire));
        assert!(state.worker.lock().unwrap().is_closing());
    }
}

#[cfg(test)]
fn ipc_view() -> WaveformView {
    WaveformView {
        generation: crate::ipc_number::MAX_SAFE_INTEGER,
        running: false,
        error: None,
        duration_seconds: 10.,
        container_origin_seconds: -0.25,
        audio_stream_index: Some(1),
        start_seconds: 0.,
        seconds_per_peak: 0.01,
        peaks: vec![[-0.5, 0.75], [0., 0.]],
    }
}

#[cfg(test)]
pub(crate) fn ipc_sample() -> serde_json::Value {
    serde_json::to_value(ipc_view()).unwrap()
}

#[test]
fn ipc_waveform_rejects_nonfinite_samples_instead_of_null_peaks() {
    let mut view = ipc_view();
    view.peaks[0][1] = f32::INFINITY;
    assert!(serde_json::to_value(&view).is_err());
    view.peaks.clear();
    view.duration_seconds = f64::NAN;
    assert!(serde_json::to_value(&view).is_err());
}
