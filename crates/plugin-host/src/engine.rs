use crate::{Failure, MAX_MESSAGE_BYTES, Request, protocol::MAX_DIAGNOSTIC_BYTES};
use rquickjs::{Context, Ctx, Function, Runtime, Value};
use serde_json::value::RawValue;
use std::time::{Duration, Instant};

pub(crate) fn execute(request: &Request) -> Result<Box<RawValue>, Failure> {
    let deadline = Instant::now() + Duration::from_millis(u64::from(request.limits.timeout_ms));
    let runtime = Runtime::new().map_err(|e| Failure::Worker(e.to_string()))?;
    // The default C allocator is intentional: custom Rust allocators bypass
    // QuickJS's heap limit. No loader, native modules or host capabilities.
    runtime.set_memory_limit(request.limits.heap_bytes);
    runtime.set_max_stack_size(request.limits.stack_bytes);
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));
    let context = Context::full(&runtime).map_err(|e| Failure::Worker(e.to_string()))?;
    let result = context.with(|ctx| {
        let run = || -> Result<Box<RawValue>, Failure> {
            let input = serde_json::to_vec(&request.input)
                .map_err(|e| Failure::InvalidRequest(e.to_string()))?;
            let input = ctx
                .json_parse(input)
                .map_err(|e| failure(&ctx, e, deadline))?;
            let function: Function = ctx
                .eval(format!(
                    "(function(input){{\n'use strict';\n{}\n}})",
                    request.script
                ))
                .map_err(|e| failure(&ctx, e, deadline))?;
            let value: Value = function
                .call((input,))
                .map_err(|e| failure(&ctx, e, deadline))?;
            if value.is_promise() {
                return Err(Failure::Script(
                    "Asynchronous plugin results are unsupported".into(),
                ));
            }
            let json = ctx
                .json_stringify(value)
                .map_err(|e| failure(&ctx, e, deadline))?
                .ok_or_else(|| Failure::Script("Plugin must return a JSON value".into()))?;
            let bytes = json.to_cstring().map_err(|e| failure(&ctx, e, deadline))?;
            if bytes.len() > MAX_MESSAGE_BYTES {
                return Err(Failure::Protocol(
                    "Plugin result exceeds its byte limit".into(),
                ));
            }
            // Keep one validated JSON value for the reply envelope. Building a
            // Rust object tree here only to serialize it again wastes bounded
            // execution time; the parent still parses the complete reply into
            // Value before application validation. No engine value escapes.
            RawValue::from_string(bytes.as_str().to_owned())
                .map_err(|e| Failure::Protocol(e.to_string()))
        };
        run()
    });
    if Instant::now() >= deadline {
        return Err(Failure::Timeout);
    }
    // No event loop runs, so promises cannot secretly continue after a result.
    if runtime.is_job_pending() {
        return Err(Failure::Script(
            "Asynchronous plugin jobs are unsupported".into(),
        ));
    }
    result
}

fn failure(ctx: &Ctx<'_>, error: rquickjs::Error, deadline: Instant) -> Failure {
    if Instant::now() >= deadline {
        return Failure::Timeout;
    }
    if !error.is_exception() {
        return Failure::Script(error.to_string());
    }
    let thrown = ctx.catch();
    // Do not coerce arbitrary objects or allocate unbounded Rust diagnostics.
    // A message getter still runs under both engine and parent deadlines.
    let message = thrown.as_string().cloned().or_else(|| {
        thrown
            .as_object()?
            .get::<_, rquickjs::String>("message")
            .ok()
    });
    let message = message.and_then(|s| s.to_cstring().ok()).map(|s| {
        let text = s.as_str();
        text[..text.floor_char_boundary(MAX_DIAGNOSTIC_BYTES.min(text.len()))].to_owned()
    });
    if Instant::now() >= deadline {
        return Failure::Timeout;
    }
    Failure::Script(
        message.unwrap_or_else(|| "JavaScript exception or heap/stack limit exceeded".into()),
    )
}
