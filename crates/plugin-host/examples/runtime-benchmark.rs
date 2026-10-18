//! Worker transport/runtime evidence only; no subtitle validation or editor UI.
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use vsf_plugin_host::{Request, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let worker = std::env::current_exe()?
        .parent()
        .ok_or("Missing example directory")?
        .parent()
        .ok_or("Missing release directory")?
        .join(format!("vsf-plugin-worker{}", std::env::consts::EXE_SUFFIX));
    let cues: Vec<_> = (0..10_000)
        .map(|id| {
            json!({
                "id":id,"start":id*1000,"end":id*1000+900,
                "text":format!("{{\\an8\\k20}}<i>Été 世界 😀 {id}</i>\r\nSecond line")
            })
        })
        .collect();
    let input = json!(cues);
    let bytes = serde_json::to_vec(&input)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    println!(
        "fixture_ready 10000 cues, {} bytes, sha256={digest}",
        bytes.len()
    );
    io::stdout().flush()?;
    std::thread::sleep(Duration::from_millis(250));
    let mut reports = Vec::new();
    for (name, script) in [
        (
            "read-only",
            "return input.map(c => ({id:c.id, units:c.text.length, duration:c.end-c.start}));",
        ),
        (
            "proposed-data",
            "return input.map(c => ({...c,start:c.start+10,end:c.end+10}));",
        ),
    ] {
        println!("phase {name}");
        io::stdout().flush()?;
        let request = Request::new(script, input.clone());
        let mut measurements = Vec::new();
        for iteration in 0..21 {
            let start = Instant::now();
            let result = run(&worker, &[], &request, &AtomicBool::new(false))?;
            let rows = result.as_array().ok_or("Missing result rows")?;
            assert_eq!(rows.len(), 10_000);
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row["id"], index);
                if name == "read-only" {
                    assert_eq!(row["duration"], 900);
                    assert_eq!(
                        row["units"],
                        cues[index]["text"].as_str().unwrap().encode_utf16().count()
                    );
                } else {
                    assert_eq!(row["text"], cues[index]["text"]);
                    assert_eq!(row["start"], index * 1000 + 10);
                    assert_eq!(row["end"], index * 1000 + 910);
                }
            }
            if iteration > 0 {
                measurements.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        measurements.sort_by(f64::total_cmp);
        reports.push(json!({"name":name,"iterations":20,"warmups":1,"p95_ms":measurements[18],"max_ms":measurements[19],"passed":measurements[18]<=500.0}));
    }
    let passed = reports.iter().all(|r| r["passed"] == true);
    println!(
        "{}",
        json!({"schema":1,"scope":"Fresh helper, parent serialization, bounded pipes, JS execution, result decoding and all-row validation; no domain/editor/native media acceptance", "input_sha256":digest,"workloads":reports,"p95_budget_ms":500,"passed":passed})
    );
    if !passed {
        return Err("Plugin runtime CPU budget failed".into());
    }
    Ok(())
}
