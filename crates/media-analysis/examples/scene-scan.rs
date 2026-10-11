//! Reproducible production-pipeline observation; paths remain local.
use media_analysis::{
    Cancellation, Tools,
    scenes::{self, Detail},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args_os().nth(1).ok_or("Pass a media path")?);
    let start = Instant::now();
    let result = scenes::scan(
        &Tools::default(),
        &path,
        &Cancellation::default(),
        Duration::from_secs(1800),
        Detail::Standard,
        |update| {
            eprintln!(
                "{:.3}s {:?} {:.1}% {} cuts {:?}",
                start.elapsed().as_secs_f64(),
                update.phase,
                update.progress * 100.,
                update.cuts.len(),
                update.note
            );
        },
    )?;
    println!(
        "{}",
        serde_json::json!({"seconds":start.elapsed().as_secs_f64(), "cuts":result.seconds})
    );
    Ok(())
}
