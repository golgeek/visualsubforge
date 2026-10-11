//! Experimental scene appearance signatures. Blocking and independent of Tauri.
//!
//! Two sequential decode passes use the exact same cut filter: one retains PTS,
//! the other retains only 16x16 grayscale images. No full-resolution frame is
//! retained or published. The paired count is checked before returning results.
use super::{Detail, cut_filter, detect, inspect};
use crate::{Cancellation, Error, Result, Tools, process};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

pub const MAX_ANCHORS: usize = 20_000;
const PIXELS: usize = 16 * 16;
pub const MAX_SAMPLES: usize = 30_000;

#[derive(Clone, Debug)]
pub struct Anchor {
    /// Microseconds from the container origin, never a nominal frame index.
    pub time_us: u64,
    pub pixels: [u8; PIXELS],
}

/// Actual decoded frames near a half-second grid, with their original PTS.
/// They verify a cut-derived map; their sampling interval never defines speed.
pub fn sample(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Vec<Anchor>> {
    let started = Instant::now();
    let remaining = || timeout.checked_sub(started.elapsed()).ok_or(Error::Timeout);
    let source = inspect(tools, path, cancellation, remaining()?)?;
    let filter = "settb=expr=1/1000000,select='isnan(prev_selected_t)+gt(floor(t*2),floor(prev_selected_t*2))'";
    let command = |filter: &str, raw: bool| {
        let mut command = Command::new(&tools.ffmpeg);
        command
            .args(["-v", "error", "-nostdin", "-threads", "2", "-copyts", "-i"])
            .arg(&source.path)
            .args([
                "-map",
                &format!("0:{}", source.stream),
                "-an",
                "-sn",
                "-dn",
                "-filter_threads",
                "1",
                "-vf",
                filter,
                "-fps_mode",
                "passthrough",
            ]);
        if raw {
            command.args(["-c:v", "rawvideo", "-threads", "1", "-f", "rawvideo", "-"]);
        } else {
            command.args(["-f", "null", "-"]);
        }
        command
    };
    let timestamps = process::run(
        command(
            &format!(
                "{filter},metadata=mode=add:key=vsf.sample:value=1,metadata=mode=print:key=vsf.sample:file=-"
            ),
            false,
        ),
        cancellation,
        remaining()?,
        |r| process::limited(r, 8 * 1024 * 1024),
    )?;
    let times = sample_times(&timestamps, source.origin)?;
    let images = process::run(
        command(
            &format!("{filter},scale=16:16:flags=area,format=gray"),
            true,
        ),
        cancellation,
        remaining()?,
        |r| process::limited(r, (MAX_SAMPLES * PIXELS) as u64),
    )?;
    if images.len() != times.len() * PIXELS {
        return Err(Error::Message(
            "Sample timestamp/image counts disagree".into(),
        ));
    }
    // Pair BEFORE discarding pre-origin images; otherwise image/PTS order shifts.
    Ok(times
        .into_iter()
        .zip(images.as_chunks::<PIXELS>().0)
        .filter_map(|(time, pixels)| {
            u64::try_from(time).ok().map(|time_us| Anchor {
                time_us,
                pixels: *pixels,
            })
        })
        .collect())
}

fn sample_times(bytes: &[u8], origin: f64) -> Result<Vec<i64>> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Message("Invalid sample metadata".into()))?;
    let origin_us = (origin * 1_000_000.).round() as i64;
    let mut times = Vec::new();
    for line in text.lines().filter(|s| s.starts_with("frame:")) {
        let pts = line
            .split_once("pts:")
            .and_then(|(_, s)| s.split_whitespace().next())
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| Error::Message("Invalid sample PTS".into()))?;
        let time = pts
            .checked_sub(origin_us)
            .ok_or_else(|| Error::Message("Sample PTS overflow".into()))?;
        if times.len() >= MAX_SAMPLES
            || time > 14_400_000_000
            || times.last().is_some_and(|last| *last >= time)
        {
            return Err(Error::Message(
                "Invalid or excessive sample timestamps".into(),
            ));
        }
        times.push(time);
    }
    if times.is_empty() {
        return Err(Error::Message("No decoded video samples".into()));
    }
    Ok(times)
}

pub fn extract(
    tools: &Tools,
    path: &Path,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Vec<Anchor>> {
    let started = Instant::now();
    let remaining = || timeout.checked_sub(started.elapsed()).ok_or(Error::Timeout);
    let source = inspect(tools, path, cancellation, remaining()?)?;
    let cuts = detect(
        tools,
        &source.path,
        cancellation,
        remaining()?,
        Detail::Standard,
    )?;
    if cuts.seconds.len() > MAX_ANCHORS {
        return Err(Error::Message("Visual anchor count exceeds 20,000".into()));
    }
    let mut command = Command::new(&tools.ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-threads", "2", "-copyts", "-i"])
        .arg(&source.path)
        .args([
            "-map",
            &format!("0:{}", source.stream),
            "-an",
            "-sn",
            "-dn",
            "-filter_threads",
            "1",
            "-vf",
            &format!(
                "{},scale=16:16:flags=area,format=gray",
                cut_filter(Detail::Standard)
            ),
            "-fps_mode",
            "passthrough",
            "-c:v",
            "rawvideo",
            "-threads",
            "1",
            "-f",
            "rawvideo",
            "-",
        ]);
    let bytes = process::run(command, cancellation, remaining()?, |r| {
        process::limited(r, (MAX_ANCHORS * PIXELS) as u64)
    })?;
    if bytes.len() != cuts.seconds.len() * PIXELS {
        return Err(Error::Message(
            "Scene timestamp/image counts disagree".into(),
        ));
    }
    cuts.seconds
        .iter()
        .zip(bytes.as_chunks::<PIXELS>().0)
        .map(|(seconds, image)| {
            if !seconds.is_finite() || *seconds < 0. || *seconds > 14_400. {
                return Err(Error::Message("Invalid visual timestamp".into()));
            }
            Ok(Anchor {
                time_us: (seconds * 1_000_000.).round() as u64,
                pixels: *image,
            })
        })
        .collect()
}

/// Mean absolute luminance error. Values are 0..255, not probabilities.
pub(crate) fn distance(a: &Anchor, b: &Anchor) -> f64 {
    let total: u32 = a
        .pixels
        .iter()
        .zip(&b.pixels)
        .map(|(a, b)| a.abs_diff(*b) as u32)
        .sum();
    total as f64 / PIXELS as f64
}

pub(crate) fn has_detail(anchor: &Anchor) -> bool {
    let mean = anchor.pixels.iter().map(|v| u32::from(*v)).sum::<u32>() as f64 / PIXELS as f64;
    anchor
        .pixels
        .iter()
        .map(|v| (f64::from(*v) - mean).abs())
        .sum::<f64>()
        / PIXELS as f64
        >= 8.
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_metadata_keeps_zero_and_pre_origin_pair_positions() {
        let text = b"frame:0 pts:4999999 pts_time:5\nframe:1 pts:5000000 pts_time:5\nframe:2 pts:5500000 pts_time:5.5\n";
        assert_eq!(sample_times(text, 5.).unwrap(), [-1, 0, 500_000]);
        assert!(sample_times(b"frame:0 pts:NOPTS", 0.).is_err());
        assert!(sample_times(b"frame:0 pts:0\nframe:1 pts:0", 0.).is_err());
    }
}
