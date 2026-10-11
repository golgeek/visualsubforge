use media_analysis::{Cancellation, Tools, analyze};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.is_empty() || args.len() > 2 {
        return Err("Usage: media-analysis <local-media> [audio-ordinal]".into());
    }
    let audio = args
        .get(1)
        .map(|s| s.to_string_lossy().parse::<u32>())
        .transpose()?
        .unwrap_or(0);
    let tools = Tools {
        ffmpeg: std::env::var_os("VSF_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffmpeg".into()),
        ffprobe: std::env::var_os("VSF_FFPROBE")
            .map(PathBuf::from)
            .unwrap_or_else(|| "ffprobe".into()),
    };
    let start = Instant::now();
    let waveform = analyze(
        &tools,
        &PathBuf::from(&args[0]),
        audio,
        &Cancellation::default(),
        Duration::from_secs(120),
    )?;
    eprintln!(
        "Analyzed {} samples / {} peaks in {:.3}s",
        waveform.sample_count,
        waveform.peaks.len(),
        start.elapsed().as_secs_f64()
    );
    serde_json::to_writer(std::io::stdout().lock(), &waveform)?;
    Ok(())
}
