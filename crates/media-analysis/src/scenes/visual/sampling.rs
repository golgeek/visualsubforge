//! Bounded frame sampling through sequential FFmpeg passes. Timestamps and
//! pixels are paired before normalization; neither FPS nor frame indexes set time.
use super::{Anchor, PIXELS};
use crate::{
    Cancellation, Error, Result, Tools, process,
    scenes::{Source, inspect},
};
use std::{
    io::Write,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

pub const MAX_SAMPLES: usize = 30_000;

/// Dense observations and the intervals actually requested from the decoder.
/// Private fields prevent callers from labeling sparse samples as dense.
pub struct SampleWindows {
    frames: Vec<Anchor>,
    ranges: Vec<(u64, u64)>,
}

impl SampleWindows {
    pub fn frames(&self) -> &[Anchor] {
        &self.frames
    }
    pub fn covers(&self, time_us: u64, radius_us: u64) -> bool {
        let start = time_us.saturating_sub(radius_us);
        let end = time_us.saturating_add(radius_us).min(14_400_000_000);
        self.ranges
            .partition_point(|r| r.0 <= start)
            .checked_sub(1)
            .is_some_and(|i| self.ranges[i].1 >= end)
    }
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
    read_samples(tools, &source, filter, None, cancellation, remaining()?)
}

/// Decode all frames within the requested presentation-time neighborhoods.
/// Overlapping neighborhoods are merged. Returned timestamps remain actual PTS;
/// absent footage produces no samples. Both images and metadata are bounded.
/// A file-backed expression avoids command-line limits on every platform.
pub fn sample_near(
    tools: &Tools,
    path: &Path,
    requests_us: &[u64],
    radius_us: u64,
    ffmpeg_version: &str,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<SampleWindows> {
    if cancellation.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if timeout.is_zero() {
        return Err(Error::Timeout);
    }
    if radius_us > 14_400_000_000
        || requests_us.len() > MAX_SAMPLES
        || requests_us.windows(2).any(|p| p[0] >= p[1])
        || requests_us.last().is_some_and(|&t| t > 14_400_000_000)
    {
        return Err(Error::Message(
            "Invalid or excessive requested frame times".into(),
        ));
    }
    if requests_us.is_empty() {
        return Ok(SampleWindows {
            frames: Vec::new(),
            ranges: Vec::new(),
        });
    }
    let started = Instant::now();
    let remaining = || timeout.checked_sub(started.elapsed()).ok_or(Error::Timeout);
    let source = inspect(tools, path, cancellation, remaining()?)?;
    let origin = source.origin * 1_000_000.;
    if !origin.is_finite() || origin.abs() > (1u64 << 52) as f64 {
        return Err(Error::Message(
            "Container origin exceeds exact sampling range".into(),
        ));
    }
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for &time in requests_us {
        let start = time.saturating_sub(radius_us);
        let end = time.saturating_add(radius_us).min(14_400_000_000);
        if let Some(previous) = ranges.last_mut()
            && start <= previous.1
        {
            previous.1 = previous.1.max(end);
        } else {
            ranges.push((start, end));
        }
    }
    let times: Vec<_> = ranges
        .iter()
        .map(|(a, b)| {
            (
                origin.round() as i64 + *a as i64,
                origin.round() as i64 + *b as i64,
            )
        })
        .collect();
    let filter = selection(&times);
    let frames = read_samples(
        tools,
        &source,
        &filter,
        Some(filter_file_option(ffmpeg_version)?),
        cancellation,
        remaining()?,
    )?;
    Ok(SampleWindows { frames, ranges })
}

fn filter_file_option(version: &str) -> Result<&'static str> {
    let major = version
        .strip_prefix("ffmpeg version ")
        .and_then(|v| v.trim_start_matches('n').split('.').next())
        .and_then(|v| v.parse::<u32>().ok());
    // Development builds may identify themselves by Git revision rather than
    // release number. The corresponding libavutil major is still recorded.
    let utility_major = version.lines().find_map(|line| {
        line.trim()
            .strip_prefix("libavutil ")?
            .trim()
            .split('.')
            .next()?
            .parse::<u32>()
            .ok()
    });
    let modern = major
        .map(|m| m >= 7)
        .or_else(|| utility_major.map(|m| m >= 59));
    match modern {
        Some(false) => Ok("-filter_script:v"),
        Some(true) => Ok("-/filter:v"),
        _ => Err(Error::Message(
            "Cannot select frame-filter file syntax for this FFmpeg version".into(),
        )),
    }
}

fn selection(ranges: &[(i64, i64)]) -> String {
    // The disjoint ranges form a balanced search tree. FFmpeg evaluates only
    // O(log ranges) predicates per input frame, independent of clip duration.
    fn branch(ranges: &[(i64, i64)], output: &mut String) {
        use std::fmt::Write;
        if ranges.len() == 1 {
            write!(output, "between(pts,{},{})", ranges[0].0, ranges[0].1).unwrap();
        } else {
            let middle = ranges.len() / 2;
            write!(output, "if(lt(pts,{}),", ranges[middle].0).unwrap();
            branch(&ranges[..middle], output);
            output.push(',');
            branch(&ranges[middle..], output);
            output.push(')');
        }
    }
    let mut filter = "settb=expr=1/1000000,select='".to_owned();
    branch(ranges, &mut filter);
    filter.push('\'');
    filter
}

fn read_samples(
    tools: &Tools,
    source: &Source,
    filter: &str,
    file_option: Option<&str>,
    cancellation: &Cancellation,
    timeout: Duration,
) -> Result<Vec<Anchor>> {
    let started = Instant::now();
    let remaining = || timeout.checked_sub(started.elapsed()).ok_or(Error::Timeout);
    // Owned filter files outlive the supervised child, including cancellation
    // and failure. Only fixed filter syntax and checked integers enter them.
    let prepare = |suffix: &str| -> Result<Option<tempfile::NamedTempFile>> {
        if file_option.is_none() {
            return Ok(None);
        }
        let mut file = tempfile::Builder::new()
            .prefix("vsf-resync-samples-")
            .tempfile()?;
        write!(file, "{filter}{suffix}")?;
        file.flush()?;
        Ok(Some(file))
    };
    let metadata_suffix =
        ",metadata=mode=add:key=vsf.sample:value=1,metadata=mode=print:key=vsf.sample:file=-";
    let images_suffix = ",scale=16:16:flags=area,format=gray";
    let metadata_file = prepare(metadata_suffix)?;
    let images_file = prepare(images_suffix)?;
    let command = |suffix: &str, file: &Option<tempfile::NamedTempFile>, raw: bool| {
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
            ]);
        if let (Some(option), Some(file)) = (file_option, file) {
            command.arg(option).arg(file.path());
        } else {
            command.arg("-vf").arg(format!("{filter}{suffix}"));
        }
        command.args(["-fps_mode", "passthrough"]);
        if raw {
            command.args(["-c:v", "rawvideo", "-threads", "1", "-f", "rawvideo", "-"]);
        } else {
            command.args(["-f", "null", "-"]);
        }
        command
    };
    let timestamps = process::run(
        command(metadata_suffix, &metadata_file, false),
        cancellation,
        remaining()?,
        |r| process::limited(r, 8 * 1024 * 1024),
    )?;
    let times = sample_times(&timestamps, source.origin)?;
    if times.is_empty() && file_option.is_none() {
        return Err(Error::Message("No decoded video samples".into()));
    }
    let images = process::run(
        command(images_suffix, &images_file, true),
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
    Ok(times)
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

    #[test]
    fn sample_limit_rejects_an_excess_frame_instead_of_truncating_evidence() {
        use std::fmt::Write;
        let mut metadata = String::new();
        for index in 0..MAX_SAMPLES {
            writeln!(metadata, "frame:{index} pts:{index}").unwrap();
        }
        assert_eq!(
            sample_times(metadata.as_bytes(), 0.).unwrap().len(),
            MAX_SAMPLES
        );
        writeln!(metadata, "frame:{MAX_SAMPLES} pts:{MAX_SAMPLES}").unwrap();
        assert!(matches!(
            sample_times(metadata.as_bytes(), 0.),
            Err(Error::Message(message)) if message.contains("excessive sample timestamps")
        ));
    }

    #[test]
    fn dense_coverage_requires_the_entire_requested_neighborhood() {
        let windows = SampleWindows {
            frames: vec![],
            ranges: vec![(0, 1_000_000), (2_000_000, 3_000_000)],
        };
        assert!(windows.covers(0, 1_000_000));
        assert!(windows.covers(2_500_000, 500_000));
        assert!(!windows.covers(1_500_000, 0));
        assert!(!windows.covers(1_500_000, 1_000_000));
        assert!(!windows.covers(2_500_000, 500_001));
        // Requested coverage is separate from successful image correspondence;
        // an empty result at EOF must still fail image verification.
        assert!(windows.frames().is_empty());
    }

    #[test]
    fn file_filter_syntax_follows_the_recorded_tool_identity() {
        assert_eq!(
            filter_file_option("ffmpeg version 6.1.1 Copyright").unwrap(),
            "-filter_script:v"
        );
        assert_eq!(
            filter_file_option("ffmpeg version n7.0 Copyright").unwrap(),
            "-/filter:v"
        );
        assert_eq!(
            filter_file_option("ffmpeg version 9.0.2 Copyright").unwrap(),
            "-/filter:v"
        );
        assert_eq!(
            filter_file_option(
                "ffmpeg version N-1234-gabcd\nlibavutil      60.  8.100 / 60.  8.100"
            )
            .unwrap(),
            "-/filter:v"
        );
        assert!(filter_file_option("unidentified tool").is_err());
    }

    #[test]
    fn invalid_cancelled_and_empty_requests_never_start_a_tool() {
        let tools = Tools {
            ffmpeg: "missing-ffmpeg".into(),
            ffprobe: "missing-ffprobe".into(),
        };
        let path = Path::new("missing-video");
        let cancel = Cancellation::default();
        let call = |times: &[u64], timeout| {
            sample_near(
                &tools,
                path,
                times,
                100_000,
                "unidentified tool",
                &cancel,
                timeout,
            )
        };
        assert!(
            call(&[], Duration::from_secs(1))
                .unwrap()
                .frames()
                .is_empty()
        );
        for times in [
            vec![1, 0],
            vec![1, 1],
            vec![14_400_000_001],
            (0..30_001).collect(),
        ] {
            assert!(
                matches!(call(&times, Duration::from_secs(1)), Err(Error::Message(s)) if s.contains("requested frame times"))
            );
        }
        assert!(matches!(call(&[0], Duration::ZERO), Err(Error::Timeout)));
        cancel.cancel();
        assert!(matches!(
            call(&[0], Duration::from_secs(1)),
            Err(Error::Cancelled)
        ));
    }
}
