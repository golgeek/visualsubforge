use serde_json::{Value, json};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
use vsf_plugin_host::{Failure, MAX_MESSAGE_BYTES, MAX_SCRIPT_BYTES, Request, run, serve};

fn worker() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_vsf-plugin-worker"))
}
fn execute(script: &str, input: Value) -> Result<Value, Failure> {
    run(
        worker(),
        &[],
        &Request::new(script, input),
        &AtomicBool::new(false),
    )
}

#[test]
fn copied_json_unicode_and_fresh_global_lifetime() {
    let input = json!({"text":"Été 世界 😀\r\nsecond", "start": 1000});
    let before = input.clone();
    let result = execute("globalThis.previous = 7; input.start += 1; return {input, units: input.text.length, lines: input.text.split('\\r\\n').length};", input.clone()).unwrap();
    assert_eq!(input, before);
    assert_eq!(result["input"]["start"], 1001);
    assert_eq!(result["units"], 17);
    assert_eq!(result["lines"], 2);
    assert_eq!(
        execute("return typeof previous;", Value::Null).unwrap(),
        "undefined"
    );
}

#[test]
fn result_transport_preserves_json_values_and_rejects_excessive_nesting() {
    let input = json!({
        "text": "quoted \"\\\r\nÉté 世界 😀\u{2028}",
        "number": 9_007_199_254_740_991_u64,
        "nested": [{"empty": {}, "values": [true, false, null, -0.5]}],
    });
    assert_eq!(execute("return input;", input.clone()).unwrap(), input);
    // Depending on the native engine stack cost, JavaScript serialization can
    // hit its stack cap before the parent rejects JSON depth (Windows x64 does).
    // The faulty-helper probe below separately requires a parser rejection.
    let result = execute(
        "let value = 1; for (let i=0; i<160; ++i) value=[value]; return value;",
        Value::Null,
    );
    assert!(
        matches!(&result, Err(Failure::Protocol(_)))
            || matches!(&result, Err(Failure::Script(message)) if message.contains("stack")),
        "Expected a bounded nesting rejection; actual error: {:?}",
        result.err()
    );
}

#[test]
fn no_ambient_file_network_process_or_native_module_capabilities() {
    let names = json!([
        "fetch",
        "XMLHttpRequest",
        "WebSocket",
        "require",
        "process",
        "Deno",
        "Bun",
        "document",
        "std",
        "os",
        "readFile",
        "setTimeout"
    ]);
    let result = execute(
        "return input.map(name => [name, typeof globalThis[name]]);",
        names,
    )
    .unwrap();
    for row in result.as_array().unwrap() {
        assert_eq!(row[1], "undefined", "{row}");
    }
    assert!(matches!(
        execute("return import('std');", Value::Null),
        Err(Failure::Script(_))
    ));
}

#[test]
fn exceptions_and_serialization_failures_return_no_partial_result() {
    for script in [
        "input.start = 2; throw new Error('broken fix');",
        "return {bad: 1n};",
        "const a={}; a.self=a; return a;",
        "return undefined;",
        "syntax !!! error",
        "return Promise.resolve({edit: 1});",
        "Promise.resolve().then(()=>{globalThis.late=1}); return {edit:1};",
    ] {
        let input = json!({"start": 1});
        assert!(
            matches!(execute(script, input.clone()), Err(Failure::Script(_))),
            "{script}"
        );
        assert_eq!(input, json!({"start": 1}));
    }
    let Err(Failure::Script(message)) = execute("throw new Error('broken fix');", Value::Null)
    else {
        panic!("Missing script error");
    };
    assert_eq!(message, "broken fix");
    let Err(Failure::Script(message)) = execute("throw '世界'.repeat(100000);", Value::Null)
    else {
        panic!("Missing bounded error");
    };
    assert!(message.len() <= 4096);
}

#[test]
fn loops_regexp_and_serialization_are_interruptible() {
    for script in [
        "while(true) {}",
        "try { while(true) {} } catch(e) { while(true) {} }",
        "return /^(a+)+$/.test('a'.repeat(100000)+'!');",
        "return {toJSON(){while(true){}}};",
        "throw Object.defineProperty({}, 'message', {get(){while(true){}}});",
    ] {
        let mut request = Request::new(script, Value::Null);
        request.limits.timeout_ms = 150;
        let start = Instant::now();
        assert_eq!(
            run(worker(), &[], &request, &AtomicBool::new(false)),
            Err(Failure::Timeout),
            "{script}"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(execute("return 42;", Value::Null).unwrap(), 42);
    }
}

#[test]
fn recursion_and_allocations_are_bounded_and_worker_remains_reusable() {
    assert_eq!(
        execute("return new Uint8Array(32).length;", Value::Null).unwrap(),
        32
    );
    for (script, cause) in [
        (
            "function recurse(){return recurse()+1} return recurse();",
            "stack",
        ),
        (
            "const a=[]; while(true) a.push(new Uint8Array(1024*1024));",
            "memory",
        ),
    ] {
        let mut request = Request::new(script, Value::Null);
        request.limits.heap_bytes = 8 * 1024 * 1024;
        request.limits.stack_bytes = 64 * 1024;
        let Err(Failure::Script(message)) = run(worker(), &[], &request, &AtomicBool::new(false))
        else {
            panic!("Missing {cause} failure");
        };
        assert!(message.contains(cause), "{message}");
        assert_eq!(execute("return 42;", Value::Null).unwrap(), 42);
    }
}

#[test]
fn oversized_javascript_result_is_refused() {
    let mut request = Request::new("return 'x'.repeat(33*1024*1024);", Value::Null);
    request.limits.heap_bytes = 128 * 1024 * 1024;
    let started = Instant::now();
    let result = run(worker(), &[], &request, &AtomicBool::new(false));
    // On slower hosts the unchanged one-second deadline wins before the byte
    // check. A separate output-flood probe strictly exercises the byte limit.
    assert!(
        matches!(result, Err(Failure::Protocol(_) | Failure::Timeout)),
        "Expected an oversized-result rejection; actual error: {:?}",
        result.err()
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(execute("return 42;", Value::Null).unwrap(), 42);
}

#[test]
fn ten_thousand_cues_round_trip_without_per_property_ipc() {
    let input: Vec<_> = (0..10_000).map(|id| json!({"id":id, "start":id*1000, "end":id*1000+900,"text":format!("{{\\an8}}<i>Été 世界 {id}</i>")})).collect();
    let original = json!(input);
    let result = execute(
        "return input.map(cue => ({...cue, start:cue.start+10, end:cue.end+10}));",
        original.clone(),
    )
    .unwrap();
    assert_eq!(result.as_array().unwrap().len(), 10_000);
    for (index, cue) in result.as_array().unwrap().iter().enumerate() {
        assert_eq!(cue["text"], original[index]["text"]);
        assert_eq!(cue["id"], index);
        assert_eq!(cue["start"], index * 1000 + 10);
        assert_eq!(cue["end"], index * 1000 + 910);
    }
}

#[test]
fn protocol_limits_versions_and_cancellation_before_spawn() {
    let cancel = AtomicBool::new(false);
    let mut request = Request::new("return input;", Value::Null);
    for limits in [0, 10_001] {
        request.limits.timeout_ms = limits;
        assert!(matches!(
            run(worker(), &[], &request, &cancel),
            Err(Failure::InvalidRequest(_))
        ));
    }
    request.limits.timeout_ms = 1000;
    for heap in [0, 128 * 1024 * 1024 + 1] {
        request.limits.heap_bytes = heap;
        assert!(matches!(
            run(worker(), &[], &request, &cancel),
            Err(Failure::InvalidRequest(_))
        ));
    }
    request.limits.heap_bytes = 64 * 1024 * 1024;
    for stack in [0, 512 * 1024 + 1] {
        request.limits.stack_bytes = stack;
        assert!(matches!(
            run(worker(), &[], &request, &cancel),
            Err(Failure::InvalidRequest(_))
        ));
    }
    request.limits.stack_bytes = 256 * 1024;
    request.version = 2;
    assert!(matches!(
        run(worker(), &[], &request, &cancel),
        Err(Failure::InvalidRequest(_))
    ));
    request.version = 1;
    request.script = "a".repeat(MAX_SCRIPT_BYTES + 1);
    assert!(matches!(
        run(worker(), &[], &request, &cancel),
        Err(Failure::InvalidRequest(_))
    ));
    request.script = "return input;".into();
    request.input = json!("x".repeat(MAX_MESSAGE_BYTES));
    assert!(matches!(
        run(worker(), &[], &request, &cancel),
        Err(Failure::InvalidRequest(_))
    ));
    request.input = Value::Null;
    cancel.store(true, Ordering::Release);
    assert_eq!(
        run(Path::new("nonexistent-worker"), &[], &request, &cancel),
        Err(Failure::Cancelled)
    );
    for bytes in [
        b"not json".to_vec(),
        b"{}".to_vec(),
        b"{\"version\":999}".to_vec(),
        vec![b' '; MAX_MESSAGE_BYTES + 1],
    ] {
        let mut output = Vec::new();
        serve(bytes.as_slice(), &mut output).unwrap();
        let reply: Value = serde_json::from_slice(&output).unwrap();
        assert!(reply["result"]["Err"].is_object());
        assert!(output.len() < 5000);
    }
    for (field, value) in [
        ("version", json!(2)),
        ("unknown", json!(true)),
        (
            "limits",
            json!({"timeout_ms":1000,"heap_bytes":64*1024*1024,"stack_bytes":256*1024,"unknown":true}),
        ),
    ] {
        let mut message = serde_json::to_value(&request).unwrap();
        message[field] = value;
        let bytes = serde_json::to_vec(&message).unwrap();
        let mut output = Vec::new();
        serve(bytes.as_slice(), &mut output).unwrap();
        let reply: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(reply["result"]["Err"]["kind"], "invalidRequest");
    }
}

fn probe(directory: &Path) -> PathBuf {
    let binary = directory.join(format!("probe{}", std::env::consts::EXE_SUFFIX));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/worker_probe.rs");
    let output = Command::new("rustc")
        .arg("--edition=2024")
        .arg(source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    binary
}

fn assert_reaped(pid_path: &Path) {
    let pid = std::fs::read_to_string(pid_path).unwrap();
    #[cfg(unix)]
    assert!(
        !Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success(),
        "Child {pid} survived"
    );
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args([
                "/FI",
                &format!("PID eq {}", pid.trim()),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(&format!("\"{}\"", pid.trim())));
    }
}

#[test]
fn parent_watchdog_bounds_blocked_pipes_floods_crash_and_invalid_protocol() {
    let directory = tempfile::tempdir().unwrap();
    let binary = probe(directory.path());
    for mode in [
        "no-read",
        "after-reply",
        "output-flood",
        "stderr-flood",
        "bad-json",
        "nested-json",
        "deep-json",
        "wrong-version",
        "crash",
    ] {
        let pid = directory.path().join(format!("{mode}.pid"));
        let mut request = Request::new(" ".repeat(512 * 1024), Value::Null);
        request.limits.timeout_ms = 500;
        let args = [OsString::from(mode), pid.clone().into_os_string()];
        let start = Instant::now();
        let result = run(&binary, &args, &request, &AtomicBool::new(false));
        if matches!(mode, "no-read" | "after-reply") {
            assert_eq!(result, Err(Failure::Timeout));
        } else if mode == "crash" {
            assert!(matches!(result, Err(Failure::Worker(_))), "{result:?}");
        } else if mode == "nested-json" {
            let mut expected = json!(1);
            for _ in 0..32 {
                expected = json!([expected]);
            }
            assert_eq!(result, Ok(expected));
        } else if mode == "deep-json" {
            assert!(
                matches!(&result, Err(Failure::Protocol(message)) if message.contains("recursion limit")),
                "Expected the parent's parser depth limit: {result:?}"
            );
        } else {
            assert!(
                matches!(result, Err(Failure::Protocol(_))),
                "{mode}: {result:?}"
            );
        }
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_reaped(&pid);
    }
}

#[test]
fn cancellation_kills_and_reaps_a_live_helper_with_blocked_input() {
    let directory = tempfile::tempdir().unwrap();
    let binary = probe(directory.path());
    let pid = directory.path().join("cancel.pid");
    let cancel = AtomicBool::new(false);
    let mut request = Request::new(" ".repeat(512 * 1024), Value::Null);
    request.limits.timeout_ms = 2000;
    thread::scope(|scope| {
        scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(3);
            while !pid.exists() {
                assert!(Instant::now() < end, "Helper did not start");
                thread::sleep(Duration::from_millis(5));
            }
            thread::sleep(Duration::from_millis(30));
            cancel.store(true, Ordering::Release);
        });
        let args = [OsString::from("no-read"), pid.clone().into_os_string()];
        assert_eq!(
            run(&binary, &args, &request, &cancel),
            Err(Failure::Cancelled)
        );
    });
    assert_reaped(&pid);
    assert_eq!(execute("return 42;", Value::Null).unwrap(), 42);
}
