//! Portable compressed-header candidates via FFmpeg, followed by image checks.
//! H.264 only; unsupported/malformed headers fall back to the full scan. No
//! decoder SDK, unsafe code, raw frame IPC, or standalone native helper required.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Packet {
    pts: i64,
    key: bool,
    kind: Option<u32>,
    starts: usize,
}
fn packets(reader: impl BufRead, time_base: f64) -> Result<Vec<(f64, bool)>> {
    let mut current: Option<Packet> = None;
    let mut frames = Vec::new();
    fn finish(packet: Packet, frames: &mut Vec<(f64, bool)>, tb: f64) -> Result<()> {
        if packet.starts != 1 || packet.kind.is_none() || frames.len() >= 2_000_000 {
            return Err(Error::Message("Unsupported H.264 packet layout".into()));
        }
        frames.push((
            packet.pts as f64 * tb,
            !packet.key && packet.kind == Some(2),
        ));
        Ok(())
    }
    lines(reader, |line| {
        let Some((_, text)) = line.split_once("] ") else {
            return Ok(());
        };
        if let Some(packet) = text.strip_prefix("Packet:") {
            if let Some(previous) = current.take() {
                finish(previous, &mut frames, time_base)?;
            }
            let pts = packet
                .split_once(", pts ")
                .and_then(|(_, s)| s.split(',').next())
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| Error::Message("Missing H.264 presentation time".into()))?;
            current = Some(Packet {
                pts,
                key: packet.contains("key frame"),
                ..Packet::default()
            });
        } else if let Some(packet) = &mut current {
            let field = text.split_whitespace().nth(1);
            if matches!(field, Some("first_mb_in_slice" | "slice_type")) {
                let value: u32 = text
                    .rsplit_once(" = ")
                    .and_then(|(_, s)| s.parse().ok())
                    .ok_or_else(|| Error::Message("Malformed H.264 header".into()))?;
                if field == Some("first_mb_in_slice") {
                    packet.starts += usize::from(value == 0);
                } else {
                    let kind = value % 5;
                    if value > 9 || kind > 2 || packet.kind.is_some_and(|old| old != kind) {
                        return Err(Error::Message("Unsupported H.264 slice types".into()));
                    }
                    packet.kind = Some(kind);
                }
            }
        }
        Ok(())
    })?;
    if let Some(packet) = current {
        finish(packet, &mut frames, time_base)?;
    }
    frames.sort_by(|a, b| a.0.total_cmp(&b.0));
    if frames.is_empty() || frames.windows(2).any(|w| w[0].0 >= w[1].0) {
        return Err(Error::Message(
            "Missing or ambiguous H.264 frame times".into(),
        ));
    }
    Ok(frames)
}

pub(super) fn detect(
    tools: &Tools,
    source: &Source,
    cancellation: &Cancellation,
    timeout: Duration,
    publish: &mut (impl FnMut(Update) + Send),
) -> Result<Vec<f64>> {
    if source.codec.as_deref() != Some("h264") {
        return Err(Error::Message("This codec uses the full scan".into()));
    }
    let started = Instant::now();
    let tb = source
        .time_base
        .as_deref()
        .and_then(|s| s.split_once('/'))
        .and_then(|(n, d)| Some(n.parse::<f64>().ok()? / d.parse::<f64>().ok()?))
        .filter(|t| t.is_finite() && *t > 0.)
        .ok_or_else(|| Error::Message("Unknown H.264 time base".into()))?;
    let mut command = Command::new(&tools.ffmpeg);
    command
        .args(["-v", "info", "-nostats", "-nostdin", "-copyts", "-i"])
        .arg(&source.path)
        .args([
            "-map",
            &format!("0:{}", source.stream),
            "-c:v",
            "copy",
            "-copytb",
            "1",
            "-bsf:v",
            "trace_headers",
            "-f",
            "null",
            "-",
        ]);
    let frames = process::run_stderr(command, cancellation, timeout, |r| {
        packets(BufReader::new(r), tb)
    })?;
    // Neighbor PTS supply context without a CFR/frame-rate assumption. Very sparse
    // or ambiguous neighborhoods are left to the sequential full decoder.
    let candidates: Vec<_> = frames
        .iter()
        .enumerate()
        .filter_map(|(i, &(pts, candidate))| {
            if !candidate || i < 3 || i + 3 >= frames.len() {
                return None;
            }
            let begin = (frames[i - 3].0 - source.origin - 0.001).max(0.);
            let end = frames[i + 3].0 - source.origin + 0.001;
            (end - begin <= 2. && pts > source.origin).then_some((
                pts,
                begin,
                end,
                frames[i - 2].0,
                frames[i - 1].0,
            ))
        })
        .collect();
    if candidates.len() > 2000 {
        return Err(Error::Message(
            "Too many quick candidates; using full scan".into(),
        ));
    }
    publish(Update {
        phase: Phase::Verifying,
        progress: 0.,
        cuts: vec![],
        note: None,
    });
    let workers = std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 8))
        .unwrap_or(1);
    let next = AtomicUsize::new(0);
    // A bounded channel provides backpressure; only the coordinator publishes.
    let (send, receive) = std::sync::mpsc::sync_channel(workers);
    let mut cuts = Vec::new();
    std::thread::scope(|scope| -> Result<()> {
        for _ in 0..workers {
            let send = send.clone();
            let candidates = &candidates;
            let next = &next;
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(pts, begin, end, previous2, previous1)) = candidates.get(index) else { break; };
                    if cancellation.is_cancelled() || started.elapsed() >= timeout { break; }
                    let mut command = Command::new(&tools.ffmpeg);
                    command.args(["-v", "error", "-nostdin", "-threads", "2", "-ss", &format!("{begin:.9}"), "-t", &format!("{:.9}", end-begin), "-copyts", "-i"])
                        .arg(&source.path).args(["-map", &format!("0:{}", source.stream), "-an", "-sn", "-dn", "-filter_threads", "1", "-vf", "scale=320:-2,settb=expr=1/1000000,scdet=threshold=1,metadata=mode=print:key=lavfi.scd.score:file=-", "-fps_mode", "passthrough", "-f", "null", "-"]);
                    let result = process::run(command, cancellation, timeout.saturating_sub(started.elapsed()), |reader| {
                        let mut context = [false; 2];
                        let mut matches = Vec::new();
                        scores(BufReader::new(reader), |time, score| {
                            if (time - previous2).abs() <= 0.000001 { context[0] = true; }
                            if (time - previous1).abs() <= 0.000001 { context[1] = true; }
                            if (time - pts).abs() <= 0.000001 { matches.push(score); }
                            Ok(())
                        })?;
                        Ok(context == [true, true] && matches.len() == 1 && matches[0] >= 5.)
                    });
                    if send.send((pts - source.origin, result)).is_err() { break; }
                }
            });
        }
        drop(send);
        let mut count = 0;
        for (time, result) in receive {
            count += 1;
            if matches!(result, Ok(true)) {
                insert(&mut cuts, time)?;
            }
            // Individual seek/decode failures do not establish a cut. The full
            // decoder is still authoritative about coverage, even with zero hits.
            publish(Update {
                phase: Phase::Verifying,
                progress: count as f64 / candidates.len() as f64,
                cuts: cuts.clone(),
                note: None,
            });
        }
        if cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        // Already verified cuts remain useful even if the optional pass hits its
        // time budget; full coverage follows in every case.
        Ok(())
    })?;
    Ok(cuts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headers_use_presentation_order_and_reject_ambiguous_packets() {
        let text = "[trace_headers @ x] Packet: 5 bytes, key frame, pts 5, dts 3, duration 1.\n[trace_headers @ x] 8 first_mb_in_slice 1 = 0\n[trace_headers @ x] 9 slice_type 1 = 7\n[trace_headers @ x] Packet: 5 bytes, pts 7, dts 4, duration 1.\n[trace_headers @ x] 8 first_mb_in_slice 1 = 0\n[trace_headers @ x] 9 slice_type 1 = 7\n[trace_headers @ x] Packet: 5 bytes, pts 6, dts 5, duration 1.\n[trace_headers @ x] 8 first_mb_in_slice 1 = 0\n[trace_headers @ x] 9 slice_type 1 = 6\n";
        assert_eq!(
            packets(text.as_bytes(), 0.5).unwrap(),
            vec![(2.5, false), (3., false), (3.5, true)]
        );
        assert!(packets(text.replace("pts 6", "pts 7").as_bytes(), 1.).is_err());
        assert!(
            packets(
                text.replace("slice_type 1 = 7", "slice_type 1 = 3")
                    .as_bytes(),
                1.
            )
            .is_err()
        );
        assert!(packets(b"".as_slice(), 1.).is_err());
    }
}
