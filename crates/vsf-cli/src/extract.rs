use crate::args::Extract;
use media_analysis::{Cancellation, Tools, subtitle_tracks};
use std::{
    io::{self, Write},
    time::Duration,
};
use vsf_application::subtitle_extraction;
pub fn run(args: Extract, cancel: &Cancellation) -> Result<u8, (u8, String)> {
    let operation = || -> Result<(), String> {
        let tools = Tools::discover().map_err(|e| e.to_string())?;
        let timeout = Duration::from_secs(args.timeout_seconds);
        let text = if args.list {
            let listing = subtitle_tracks::list(&tools, &args.video, cancel, timeout)
                .map_err(|e| e.to_string())?;
            if args.json {
                serde_json::to_string(&serde_json::json!({"schema_version": 1, "tracks": listing.tracks, "font_count": listing.font_count})).map_err(|e| e.to_string())?
            } else {
                listing
                    .tracks
                    .iter()
                    .map(|t| {
                        format!(
                            "{}\t{}\t{}\t{}",
                            t.index,
                            t.codec,
                            t.language.as_deref().unwrap_or(""),
                            t.unavailable.as_deref().unwrap_or("Available")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        } else {
            let output = args.output.as_deref().ok_or("Output is required")?;
            // Refuse existing paths before expensive demuxing; publication checks again.
            vsf_application::new_output::resolve(output).map_err(|e| e.to_string())?;
            let prepared = subtitle_extraction::extract(
                &tools,
                &args.video,
                args.track.ok_or("Track is required")?,
                cancel,
                timeout,
            )?;
            subtitle_extraction::publish(&prepared, output, cancel)?;
            if args.json {
                serde_json::json!({"schema_version":1,"cue_count":prepared.parsed.document.cues().len(),"output":output,"repaired_cues":prepared.repaired_cues}).to_string()
            } else {
                format!(
                    "Extracted {} cues to {}",
                    prepared.parsed.document.cues().len(),
                    output.display()
                )
            }
        };
        writeln!(io::stdout().lock(), "{text}").map_err(|e| e.to_string())
    };
    operation()
        .map(|()| 0)
        .map_err(|e| (if cancel.is_cancelled() { 130 } else { 2 }, e))
}
