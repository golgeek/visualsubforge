//! Hard-cut candidates, in seconds relative to the same container origin as audio.
//! Blocks on supervised FFmpeg children; callers must use an analysis worker.
mod quick;
pub mod visual;

use crate::{Cancellation, Error, Result, Tools, process};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Each detail level identifies fixed resolution/threshold semantics in saved
/// projects. Decoder thread count does not change those semantics.
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Detail {
    #[default]
    Standard,
    Reduced,
}
impl Detail {
    pub fn analysis_version(self) -> u32 {
        match self {
            Self::Standard => 1,
            Self::Reduced => 2,
        }
    }
    fn scale(self) -> &'static str {
        match self {
            Self::Standard => "scale=w='min(640,iw)':h=-2",
            Self::Reduced => "scale=w=-2:h='min(64,ih)':flags=bilinear",
        }
    }
}

#[derive(Debug)]
pub struct SceneCuts {
    pub seconds: Vec<f64>,
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
    time_base: Option<String>,
}
#[derive(Deserialize)]
struct Format {
    start_time: Option<String>,
    duration: Option<String>,
}

pub fn detect(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
    detail: Detail,
) -> Result<SceneCuts> {
    // Frame dependencies can leave decoder workers waiting: allow two per available
    // CPU, capped at 32 to bound buffering/thread overhead. This is a provisional
    // portable heuristic, not per-file autotuning; see task 008 measurements.
    let threads = std::thread::available_parallelism()
        .map(|count| count.get().saturating_mul(2).min(32))
        .unwrap_or(2);
    detect_with_threads(tools, path, cancellation, timeout, detail, threads)
}

fn detect_with_threads(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
    detail: Detail,
    threads: usize,
) -> Result<SceneCuts> {
    let started = Instant::now();
    let Source {
        path,
        stream,
        origin,
        ..
    } = inspect(tools, path, cancellation, timeout)?;
    let mut decode = Command::new(&tools.ffmpeg);
    let threads = threads.to_string();
    decode
        .args([
            "-v", "error", "-nostdin", "-threads", &threads, "-copyts", "-i",
        ])
        .arg(&path);
    let filter = format!(
        "{},metadata=mode=print:key=lavfi.scd.time:file=-",
        cut_filter(detail)
    );
    // Downscale every decoded image; no temporal sampling or keyframe filtering.
    // Keep original PTS and avoid upscaling small sources. This reduces analysis
    // work, not the source decoder's workload. Software decoding is portable.
    decode.args([
        "-map",
        &format!("0:{}", stream),
        "-an",
        "-sn",
        "-dn",
        "-filter_threads",
        "1",
        "-vf",
        &filter,
        "-fps_mode",
        "passthrough",
        "-f",
        "null",
        "-",
    ]);
    let remaining = timeout
        .checked_sub(started.elapsed())
        .ok_or(Error::Timeout)?;
    let bytes = process::run(decode, cancellation, remaining, |r| {
        process::limited(r, 16 * 1024 * 1024)
    })?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::Message("Invalid scene detector output".into()))?;
    Ok(SceneCuts {
        seconds: parse(text, origin)?,
    })
}

// The timestamp and image passes must select exactly the same decoded frames.
fn cut_filter(detail: Detail) -> String {
    format!(
        "{},settb=expr=1/1000000,scdet=threshold=10:sc_pass=1",
        detail.scale()
    )
}
/// Pipeline snapshots contain metadata only. The caller must reject stale jobs.
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Candidates,
    Verifying,
    Full,
}
#[derive(Debug)]
pub struct Update {
    pub phase: Phase,
    pub progress: f64,
    pub cuts: Vec<f64>,
    pub note: Option<String>,
}
struct Source {
    path: PathBuf,
    stream: u32,
    origin: f64,
    duration: f64,
    codec: Option<String>,
    time_base: Option<String>,
}

/// Quick, image-verified non-key I pictures followed by a gentle whole-video
/// scan. Neither absence nor presence of I candidates establishes coverage.
/// Callbacks run on analysis/pipe-reader workers, never on the UI thread.
pub fn scan(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
    detail: Detail,
    mut publish: impl FnMut(Update) + Send,
) -> Result<SceneCuts> {
    let started = Instant::now();
    let source = inspect(tools, path, cancellation, timeout)?;
    publish(Update {
        phase: Phase::Candidates,
        progress: 0.,
        cuts: vec![],
        note: None,
    });
    let quick_budget = timeout
        .saturating_sub(started.elapsed())
        .min(Duration::from_secs(30));
    let (mut cuts, note) =
        match quick::detect(tools, &source, cancellation, quick_budget, &mut publish) {
            Ok(cuts) => (cuts, None),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(error) => (vec![], Some(format!("Quick scan unavailable: {error}"))),
        };
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    publish(Update {
        phase: Phase::Full,
        progress: 0.,
        cuts: cuts.clone(),
        note: note.clone(),
    });
    let mut command = Command::new(&tools.ffmpeg);
    // Bound background CPU pressure, independent of the short parallel quick pass.
    // Two decoder threads + one filter thread; this is not a hard CPU quota.
    command
        .args(["-v", "error", "-nostdin", "-threads", "2", "-copyts", "-i"])
        .arg(&source.path);
    let filter = format!(
        "{},settb=expr=1/1000000,scdet=threshold=10,metadata=mode=print:key=lavfi.scd.score:file=-:direct=1",
        detail.scale()
    );
    command.args([
        "-map",
        &format!("0:{}", source.stream),
        "-an",
        "-sn",
        "-dn",
        "-filter_threads",
        "1",
        "-vf",
        &filter,
        "-fps_mode",
        "passthrough",
        "-f",
        "null",
        "-",
    ]);
    let remaining = timeout
        .checked_sub(started.elapsed())
        .ok_or(Error::Timeout)?;
    process::run(command, cancellation, remaining, |reader| {
        let mut last = Instant::now();
        let mut through: f64 = 0.;
        scores(BufReader::new(reader), |pts, score| {
            let seconds = pts - source.origin;
            through = through.max(seconds);
            if seconds > 0. && seconds <= 14_400. && score >= 10. {
                insert(&mut cuts, seconds)?;
            }
            if last.elapsed() >= Duration::from_millis(200) {
                publish(Update {
                    phase: Phase::Full,
                    progress: (through / source.duration).clamp(0., 0.999),
                    cuts: cuts.clone(),
                    note: note.clone(),
                });
                last = Instant::now();
            }
            Ok(())
        })
    })?;
    publish(Update {
        phase: Phase::Full,
        progress: 1.,
        cuts: cuts.clone(),
        note,
    });
    Ok(SceneCuts { seconds: cuts })
}

fn insert(cuts: &mut Vec<f64>, time: f64) -> Result<()> {
    let index = cuts.partition_point(|t| *t < time - 0.000001);
    if cuts
        .get(index)
        .is_some_and(|t| (*t - time).abs() <= 0.000001)
    {
        return Ok(());
    }
    if cuts.len() >= 100_000 {
        return Err(Error::Message("Scene count exceeds limit".into()));
    }
    cuts.insert(index, time);
    Ok(())
}

// Bounded line parsing keeps per-frame progress streaming without retaining logs.
fn lines(mut reader: impl BufRead, mut visit: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let mut line = Vec::new();
    loop {
        line.clear();
        let count = Read::by_ref(&mut reader)
            .take(4097)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            return Ok(());
        }
        if count > 4096 {
            return Err(Error::Message("Scene output line exceeds limit".into()));
        }
        let line = std::str::from_utf8(&line)
            .map_err(|_| Error::Message("Invalid scene output".into()))?;
        visit(line.trim())?;
    }
}
fn scores(reader: impl BufRead, mut visit: impl FnMut(f64, f64) -> Result<()>) -> Result<()> {
    let mut pending = None;
    let mut count = 0;
    lines(reader, |line| {
        if line.starts_with("frame:") {
            if pending.is_some() {
                return Err(Error::Message("Missing scene frame score".into()));
            }
            let pts = line
                .split_once("pts:")
                .and_then(|(_, tail)| tail.split_whitespace().next())
                .and_then(|s| s.parse::<i64>().ok())
                .ok_or_else(|| Error::Message("Invalid scene PTS".into()))?;
            pending = Some(pts as f64 / 1_000_000.);
        } else if let Some(score) = line.strip_prefix("lavfi.scd.score=") {
            let pts = pending
                .take()
                .ok_or_else(|| Error::Message("Scene score without PTS".into()))?;
            count += 1;
            if count > 2_000_000 {
                return Err(Error::Message("Scene frame count exceeds limit".into()));
            }
            visit(pts, number(Some(score), "scene score")?)?;
        }
        Ok(())
    })?;
    if pending.is_some() || count == 0 {
        return Err(Error::Message("Incomplete scene output".into()));
    }
    Ok(())
}

// Shared probe retains the same stream and project-time rules in both modes.
fn inspect(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Source> {
    let path = path.canonicalize()?;
    if !path.is_file() {
        return Err(Error::Message("Media must be a local file".into()));
    }
    let mut probe = Command::new(&tools.ffprobe);
    // Uppercase V excludes cover art / attached pictures.
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            "V:0",
            "-show_entries",
            "stream=index,codec_name,time_base:format=start_time,duration",
            "-of",
            "json",
        ])
        .arg(&path);
    let bytes = process::run(probe, cancellation, timeout, |r| {
        process::limited(r, 65_536)
    })?;
    let probe: Probe = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Message(format!("Invalid scene probe: {e}")))?;
    let stream = probe
        .streams
        .first()
        .ok_or_else(|| Error::Message("This media has no video track to analyze".into()))?;
    let origin = number(probe.format.start_time.as_deref(), "container start time")?;
    let duration = number(probe.format.duration.as_deref(), "media duration")?;
    if duration <= 0.0 || duration > 14_400.0 {
        return Err(Error::Message(
            "Scene detection currently supports media up to four hours".into(),
        ));
    }
    Ok(Source {
        path,
        stream: stream.index,
        origin,
        duration,
        codec: stream.codec_name.clone(),
        time_base: stream.time_base.clone(),
    })
}

/// Fast encoded-packet candidates, not confirmed visual scene cuts.
pub fn keyframes(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<SceneCuts> {
    let started = Instant::now();
    let Source {
        path,
        stream,
        origin,
        ..
    } = inspect(tools, path, cancellation, timeout)?;
    let mut command = Command::new(&tools.ffprobe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            &stream.to_string(),
            "-show_packets",
            "-show_entries",
            "packet=pts_time,flags",
            "-of",
            "compact=p=0",
        ])
        .arg(path);
    let remaining = timeout
        .checked_sub(started.elapsed())
        .ok_or(Error::Timeout)?;
    let seconds = process::run(command, cancellation, remaining, |reader| {
        parse_packets(BufReader::new(reader), origin)
    })?;
    Ok(SceneCuts { seconds })
}
fn parse_packets(mut reader: impl BufRead, origin: f64) -> Result<Vec<f64>> {
    let mut cuts = Vec::new();
    let mut line = Vec::new();
    let mut total = 0;
    loop {
        line.clear();
        // Bound a malformed line as well as total input, without retaining every packet.
        let count = std::io::Read::by_ref(&mut reader)
            .take(4097)
            .read_until(b'\n', &mut line)?;
        if count == 0 {
            break;
        }
        total += count;
        if count > 4096 || total > 256 * 1024 * 1024 {
            return Err(Error::Message(
                "Keyframe packet output exceeds limit".into(),
            ));
        }
        let text = std::str::from_utf8(&line)
            .map_err(|_| Error::Message("Invalid keyframe output".into()))?;
        let fields: Vec<_> = text.trim().split('|').collect();
        if !fields.iter().any(|field| {
            field
                .strip_prefix("flags=")
                .is_some_and(|flags| flags.contains('K'))
        }) {
            continue;
        }
        let pts = fields
            .iter()
            .find_map(|field| field.strip_prefix("pts_time="));
        let seconds = number(pts, "keyframe presentation timestamp")? - origin;
        if seconds < 0.0 {
            continue;
        }
        if cuts.len() >= 100_000 {
            return Err(Error::Message(
                "Keyframe count exceeds 100,000 limit".into(),
            ));
        }
        cuts.push(seconds);
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    Ok(cuts)
}
fn number(value: Option<&str>, label: &str) -> Result<f64> {
    value
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite())
        .ok_or_else(|| Error::Message(format!("Unknown or invalid {label}")))
}
fn parse(text: &str, origin: f64) -> Result<Vec<f64>> {
    let mut cuts = Vec::new();
    for line in text.lines().filter(|s| s.starts_with("frame:")) {
        // Parse integer microsecond PTS, not FFmpeg's rounded display-time text.
        let pts = line
            .split_once("pts:")
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| Error::Message("Invalid scene frame timestamp".into()))?;
        let seconds = pts as f64 / 1_000_000.0 - origin;
        if seconds <= 0.0 {
            continue;
        }
        if cuts.len() >= 100_000 {
            return Err(Error::Message("Scene count exceeds 100,000 limit".into()));
        }
        cuts.push(seconds);
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    Ok(cuts)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_pts_map_offsets_sort_and_deduplicate_without_fps_assumptions() {
        let cuts = parse("frame:0 pts:5123456 pts_time:5.12346\nlavfi.scd.time=5.12346\nframe:1 pts:6000000 pts_time:6\nframe:2 pts:5123456 pts_time:5.12346\nframe:3 pts:4999999 pts_time:5", 5.0).unwrap();
        assert!((cuts[0] - 0.123456).abs() < 1e-10);
        assert_eq!(cuts.len(), 2);
        assert_eq!(cuts[1], 1.0);
    }
    #[test]
    fn packet_candidates_use_pts_flags_origin_and_bounded_input() {
        let text = b"pts_time=5.000000|flags=K__\npts_time=6.000000|flags=___\npts_time=7.500000|flags=K__\npts_time=5.000000|flags=K__\n";
        assert_eq!(parse_packets(&text[..], 5.).unwrap(), vec![0., 2.5]);
        assert!(parse_packets(&b"pts_time=N/A|flags=K__\n"[..], 0.).is_err());
        assert!(parse_packets(&vec![b'x'; 5000][..], 0.).is_err());
    }
    #[test]
    fn empty_video_has_no_cuts_and_malformed_pts_fail() {
        assert!(parse("", 0.0).unwrap().is_empty());
        assert!(parse("frame:0 pts:NOPTS pts_time:NOPTS", 0.0).is_err());
    }
}
