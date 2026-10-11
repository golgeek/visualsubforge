//! Bounded FFmpeg analysis experiment, independent of Tauri and subtitle-core.
//! `analyze` blocks: call it on a worker, never on a UI/event-loop thread.
pub mod alignment;
mod process;
pub mod scenes;

use serde::{Deserialize, Serialize};
use std::{
    fmt,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Cancelled,
    Timeout,
    Message(String),
    Io(std::io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Analysis cancelled"),
            Self::Timeout => write!(f, "Analysis timed out"),
            Self::Message(message) => f.write_str(message),
            Self::Io(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// This experiment uses 8 kHz mono samples and 10 ms peak buckets.
/// Integer sample counts avoid cumulative floating-point timeline drift.
pub const SAMPLE_RATE: u32 = 8_000;
pub const SAMPLES_PER_PEAK: u32 = 80;
const MAX_SAMPLES: u64 = SAMPLE_RATE as u64 * 4 * 60 * 60;

#[derive(Debug, Serialize)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}
#[derive(Debug, Serialize)]
pub struct Waveform {
    pub container_origin_seconds: f64,
    pub audio_stream_index: u32,
    pub sample_rate: u32,
    pub samples_per_peak: u32,
    pub sample_count: u64,
    pub peaks: Vec<Peak>,
}

pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}
impl Default for Tools {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
        }
    }
}
#[derive(Deserialize)]
struct Probe {
    streams: Vec<Stream>,
    format: Format,
}
#[derive(Deserialize)]
struct Stream {
    index: u32,
}
#[derive(Deserialize)]
struct Format {
    start_time: Option<String>,
}

/// Selects the zero-based AUDIO ordinal, not the global stream index.
/// Project time zero is the container start time. FFmpeg preserves timestamps,
/// shifts them to that origin, then fills audio gaps and trims pre-origin samples.
/// Returns no partial waveform on cancellation, timeout, decode failure or >4h input.
pub fn analyze(
    tools: &Tools,
    path: &Path,
    audio_ordinal: u32,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Waveform> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(Error::Message("Media must be a local file".into()));
    }
    let start = std::time::Instant::now();
    let mut probe = Command::new(&tools.ffprobe);
    probe.args([
        "-v",
        "error",
        "-select_streams",
        &format!("a:{audio_ordinal}"),
        "-show_entries",
        "stream=index:format=start_time",
        "-of",
        "json",
    ]);
    probe.arg(&path);
    let bytes = process::run(probe, cancellation, timeout, |r| {
        process::limited(r, 65_536)
    })?;
    let probe: Probe = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Message(format!("Invalid ffprobe response: {e}")))?;
    let stream = probe
        .streams
        .first()
        .ok_or_else(|| Error::Message(format!("Audio track {audio_ordinal} does not exist")))?;
    let origin: f64 = probe
        .format
        .start_time
        .as_deref()
        .ok_or_else(|| Error::Message("Container start time is unknown".into()))?
        .parse()
        .map_err(|_| Error::Message("Invalid container start time".into()))?;
    if !origin.is_finite() {
        return Err(Error::Message("Non-finite container start time".into()));
    }
    let mut decode = Command::new(&tools.ffmpeg);
    decode.args(["-v", "error", "-nostdin", "-copyts", "-i"]);
    decode.arg(&path);
    decode.args([
        "-map",
        &format!("0:{}", stream.index),
        "-vn",
        "-sn",
        "-dn",
        "-ac",
        "1",
        "-af",
        &format!("asetpts=PTS-({origin})/TB,aresample={SAMPLE_RATE}:async=1:first_pts=0"),
        "-ar",
        "8000",
        "-f",
        "f32le",
        "pipe:1",
    ]);
    let remaining = timeout.checked_sub(start.elapsed()).ok_or(Error::Timeout)?;
    let (sample_count, peaks) = process::run(decode, cancellation, remaining, |r| {
        peaks(BufReader::new(r))
    })?;
    if sample_count == 0 {
        return Err(Error::Message("Audio track decoded no samples".into()));
    }
    Ok(Waveform {
        container_origin_seconds: origin,
        audio_stream_index: stream.index,
        sample_rate: SAMPLE_RATE,
        samples_per_peak: SAMPLES_PER_PEAK,
        sample_count,
        peaks,
    })
}

fn peaks(reader: impl Read) -> Result<(u64, Vec<Peak>)> {
    bounded_peaks(reader, MAX_SAMPLES)
}

fn bounded_peaks(mut reader: impl Read, max_samples: u64) -> Result<(u64, Vec<Peak>)> {
    let mut result = Vec::new();
    let mut count = 0;
    let mut bucket = Peak {
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
    };
    loop {
        let mut bytes = [0; 4];
        // Reading one byte first distinguishes clean EOF from a truncated sample.
        if reader.read(&mut bytes[..1])? == 0 {
            break;
        }
        reader.read_exact(&mut bytes[1..])?;
        let sample = f32::from_le_bytes(bytes);
        if !sample.is_finite() {
            return Err(Error::Message("Non-finite PCM sample".into()));
        }
        if count == max_samples {
            return Err(Error::Message(
                "Waveform exceeds four-hour experiment limit".into(),
            ));
        }
        bucket.min = bucket.min.min(sample);
        bucket.max = bucket.max.max(sample);
        count += 1;
        if count % u64::from(SAMPLES_PER_PEAK) == 0 {
            result.push(bucket);
            bucket = Peak {
                min: f32::INFINITY,
                max: f32::NEG_INFINITY,
            };
        }
    }
    if count % u64::from(SAMPLES_PER_PEAK) != 0 {
        result.push(bucket);
    }
    Ok((count, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_bucket_preserves_extrema_without_inventing_zero() {
        let values: Vec<u8> = [0.25_f32; 81]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let (count, peaks) = peaks(values.as_slice()).unwrap();
        assert_eq!(count, 81);
        assert_eq!(peaks.len(), 2);
        assert_eq!(peaks[1].min, 0.25);
        assert_eq!(peaks[1].max, 0.25);
    }
    #[test]
    fn sample_limit_rejects_instead_of_returning_truncated_audio() {
        let samples: Vec<u8> = [0.0_f32; 81]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        assert!(bounded_peaks(samples.as_slice(), 80).is_err());
        assert_eq!(bounded_peaks(samples.as_slice(), 81).unwrap().0, 81);
    }

    #[test]
    fn malformed_samples_are_rejected() {
        assert!(peaks(&[0_u8; 3][..]).is_err());
        assert!(peaks(f32::NAN.to_le_bytes().as_slice()).is_err());
    }
    #[test]
    fn cancellation_is_shared_between_threads() {
        let cancellation = Cancellation::default();
        let other = cancellation.clone();
        std::thread::spawn(move || other.cancel()).join().unwrap();
        assert!(cancellation.is_cancelled());
    }
}
