//! Bounded FFmpeg analysis experiment, independent of Tauri and subtitle-core.
//! `analyze` blocks: call it on a worker, never on a UI/event-loop thread.
pub mod alignment;
mod process;
pub mod scenes;
pub mod tools;

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
    codec_name: Option<String>,
}
#[derive(Deserialize)]
struct Format {
    start_time: Option<String>,
    format_name: Option<String>,
    nb_streams: Option<u32>,
}

enum Origin {
    Container(f64),
    VerifyPcmWav,
}

impl Probe {
    fn origin(&self) -> Result<Origin> {
        if let Some(start) = &self.format.start_time {
            let origin: f64 = start
                .parse()
                .map_err(|_| Error::Message("Invalid container start time".into()))?;
            if !origin.is_finite() {
                return Err(Error::Message("Non-finite container start time".into()));
            }
            return Ok(Origin::Container(origin));
        }
        if self.format.format_name.as_deref() == Some("wav")
            && self.format.nb_streams == Some(1)
            && self.streams.len() == 1
            && self.streams[0]
                .codec_name
                .as_deref()
                .is_some_and(|name| name.starts_with("pcm_"))
        {
            return Ok(Origin::VerifyPcmWav);
        }
        Err(Error::Message("Container start time is unknown".into()))
    }
}

fn verified_wav_sample_zero(bytes: &[u8], stream_index: u32) -> Result<f64> {
    #[derive(Deserialize)]
    struct Packets {
        #[serde(default)]
        packets: Vec<Packet>,
    }
    #[derive(Deserialize)]
    struct Packet {
        stream_index: u32,
        pts: Option<i64>,
    }
    let response: Packets = serde_json::from_slice(bytes)
        .map_err(|e| Error::Message(format!("Invalid ffprobe packet response: {e}")))?;
    if matches!(response.packets.as_slice(), [packet]
        if packet.stream_index == stream_index && packet.pts == Some(0))
    {
        Ok(0.0)
    } else {
        Err(Error::Message(
            "PCM WAV first audio packet has no verified zero timestamp".into(),
        ))
    }
}

/// Selects the zero-based AUDIO ordinal, not the global stream index.
/// Project time zero is the container start time. FFmpeg preserves timestamps,
/// shifts them to that origin, then fills audio gaps and trims pre-origin samples.
/// If a single-stream PCM WAV omits its origin, verify its first packet PTS is zero.
/// Recording metadata such as BWF time_reference is not a playback timestamp.
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
        "stream=index,codec_name:format=start_time,format_name,nb_streams",
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
    let origin = match probe.origin()? {
        Origin::Container(origin) => origin,
        Origin::VerifyPcmWav => {
            // WAV can omit format.start_time. Verify demuxed sample zero rather
            // than guessing an origin for every format with missing metadata.
            // One packet and the original job deadline bound this extra probe.
            let mut packet = Command::new(&tools.ffprobe);
            packet.args([
                "-v",
                "error",
                "-select_streams",
                &format!("a:{audio_ordinal}"),
                "-read_intervals",
                "%+#1",
                "-show_entries",
                "packet=stream_index,pts",
                "-of",
                "json",
            ]);
            packet.arg(&path);
            let remaining = timeout.checked_sub(start.elapsed()).ok_or(Error::Timeout)?;
            let bytes = process::run(packet, cancellation, remaining, |r| {
                process::limited(r, 65_536)
            })?;
            verified_wav_sample_zero(&bytes, stream.index)?
        }
    };
    let mut decode = Command::new(&tools.ffmpeg);
    // Waveform may coexist with resync; bound its decoder/filter threads too.
    decode.args(["-v", "error", "-nostdin", "-threads", "2", "-copyts", "-i"]);
    decode.arg(&path);
    decode.args([
        "-map",
        &format!("0:{}", stream.index),
        "-vn",
        "-sn",
        "-dn",
        "-ac",
        "1",
        "-filter_threads",
        "1",
        "-af",
        &format!("asetpts=PTS-({origin})/TB,aresample={SAMPLE_RATE}:async=1:first_pts=0"),
        "-ar",
        "8000",
        "-threads",
        "1",
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

    fn wav_probe() -> serde_json::Value {
        serde_json::json!({
            "streams": [{"index": 0, "codec_name": "pcm_s16le"}],
            "format": {"format_name": "wav", "nb_streams": 1}
        })
    }

    #[test]
    fn explicit_container_origins_are_preserved_and_never_replaced_by_wav_zero() {
        for value in ["12.25", "-0.5", "0"] {
            let mut raw = wav_probe();
            raw["format"]["start_time"] = value.into();
            let probe: Probe = serde_json::from_value(raw).unwrap();
            assert!(matches!(probe.origin().unwrap(), Origin::Container(origin)
                if origin == value.parse::<f64>().unwrap()));
        }
        for value in ["", "unknown", "NaN", "inf", "-inf"] {
            let mut raw = wav_probe();
            raw["format"]["start_time"] = value.into();
            let probe: Probe = serde_json::from_value(raw).unwrap();
            assert!(probe.origin().is_err(), "{value}");
        }
    }

    #[test]
    fn missing_origin_requires_an_identified_single_stream_pcm_wav() {
        let probe: Probe = serde_json::from_value(wav_probe()).unwrap();
        assert!(matches!(probe.origin().unwrap(), Origin::VerifyPcmWav));
        for (pointer, value) in [
            ("/format/format_name", serde_json::json!("matroska,webm")),
            ("/format/format_name", serde_json::Value::Null),
            ("/format/nb_streams", serde_json::json!(2)),
            ("/format/nb_streams", serde_json::Value::Null),
            ("/streams/0/codec_name", serde_json::json!("adpcm_ms")),
            ("/streams/0/codec_name", serde_json::Value::Null),
            ("/streams", serde_json::json!([])),
            (
                "/streams",
                serde_json::json!([
                    {"index": 0, "codec_name": "pcm_s16le"},
                    {"index": 1, "codec_name": "pcm_s16le"}
                ]),
            ),
        ] {
            let mut raw = wav_probe();
            *raw.pointer_mut(pointer).unwrap() = value;
            let probe: Probe = serde_json::from_value(raw).unwrap();
            assert!(probe.origin().is_err(), "{pointer}");
        }
    }

    #[test]
    fn wav_sample_zero_requires_one_matching_packet_with_explicit_zero_pts() {
        assert_eq!(
            verified_wav_sample_zero(br#"{"packets":[{"stream_index":2,"pts":0}]}"#, 2).unwrap(),
            0.0
        );
        for bytes in [
            "{}",
            "{\"packets\":[]}",
            "{\"packets\":[{\"stream_index\":2}]}",
            "{\"packets\":[{\"stream_index\":1,\"pts\":0}]}",
            "{\"packets\":[{\"stream_index\":2,\"pts\":1}]}",
            "{\"packets\":[{\"stream_index\":2,\"pts\":-1}]}",
            "{\"packets\":[{\"stream_index\":2,\"pts\":0},{\"stream_index\":2,\"pts\":0}]}",
            "{\"packets\":[{\"stream_index\":2,\"pts\":\"bad\"}]}",
            "not JSON",
        ] {
            assert!(
                verified_wav_sample_zero(bytes.as_bytes(), 2).is_err(),
                "{bytes}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn wav_packet_verification_obeys_cancellation_and_the_overall_deadline() {
        use std::{fs, os::unix::fs::PermissionsExt, process::Stdio, thread, time::Instant};
        for cancel in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let probe = dir.path().join("ffprobe");
            fs::write(&probe, r#"#!/bin/sh
case "$*" in
  *packet=stream_index,pts*)
    printf '%s' "$$" > "$(dirname "$0")/packet.pid"
    exec sleep 60
    ;;
  *) printf '%s' '{"streams":[{"index":0,"codec_name":"pcm_s16le"}],"format":{"format_name":"wav","nb_streams":1}}' ;;
esac
"#).unwrap();
            fs::set_permissions(&probe, fs::Permissions::from_mode(0o755)).unwrap();
            let media = dir.path().join("input.wav");
            fs::write(&media, []).unwrap();
            let tools = Tools {
                ffprobe: probe,
                ffmpeg: dir.path().join("must-not-decode"),
            };
            let cancellation = Cancellation::default();
            let token = cancellation.clone();
            let started = Instant::now();
            let worker =
                thread::spawn(move || analyze(&tools, &media, 0, &token, Duration::from_secs(2)));
            let marker = dir.path().join("packet.pid");
            while !marker.exists()
                && !worker.is_finished()
                && started.elapsed() < Duration::from_secs(2)
            {
                thread::sleep(Duration::from_millis(5));
            }
            assert!(
                marker.exists(),
                "The bounded packet probe must actually start"
            );
            if cancel {
                cancellation.cancel();
            }
            let result = worker.join().unwrap();
            assert!(if cancel {
                matches!(result, Err(Error::Cancelled))
            } else {
                matches!(result, Err(Error::Timeout))
            });
            assert!(started.elapsed() < Duration::from_secs(5));
            let pid = fs::read_to_string(marker).unwrap();
            assert!(
                !Command::new("kill")
                    .args(["-0", &pid])
                    .stderr(Stdio::null())
                    .status()
                    .unwrap()
                    .success(),
                "packet probe was not reaped"
            );
        }
    }

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
