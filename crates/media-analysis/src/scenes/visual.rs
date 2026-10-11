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

#[derive(Clone, Debug)]
pub struct Anchor {
    /// Microseconds from the container origin, never a nominal frame index.
    pub time_us: u64,
    pub pixels: [u8; PIXELS],
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
