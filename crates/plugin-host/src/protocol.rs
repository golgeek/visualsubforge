use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt,
    io::{Read, Write},
};

pub const VERSION: u32 = 1;
pub const MAX_SCRIPT_BYTES: usize = 1024 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_DIAGNOSTIC_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub timeout_ms: u32,
    pub heap_bytes: usize,
    pub stack_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout_ms: 1000,
            heap_bytes: 64 * 1024 * 1024,
            stack_bytes: 256 * 1024,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    /// Body of a synchronous function receiving a JSON `input` argument.
    /// This low-level experiment is not the public plugin package/API contract.
    pub script: String,
    pub input: Value,
    pub limits: Limits,
}

impl Request {
    pub fn new(script: impl Into<String>, input: Value) -> Self {
        Self {
            version: VERSION,
            script: script.into(),
            input,
            limits: Limits::default(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), Failure> {
        if self.version != VERSION {
            return Err(Failure::InvalidRequest(
                "Unsupported plugin worker protocol version".into(),
            ));
        }
        if self.script.len() > MAX_SCRIPT_BYTES {
            return Err(Failure::InvalidRequest(
                "Plugin source exceeds 1 MiB".into(),
            ));
        }
        if !(1..=10_000).contains(&self.limits.timeout_ms)
            || !(8 * 1024 * 1024..=128 * 1024 * 1024).contains(&self.limits.heap_bytes)
            || !(64 * 1024..=512 * 1024).contains(&self.limits.stack_bytes)
        {
            return Err(Failure::InvalidRequest(
                "Plugin execution limits are outside supported bounds".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum Failure {
    InvalidRequest(String),
    Script(String),
    Protocol(String),
    Worker(String),
    Io(String),
    Timeout,
    Cancelled,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(s)
            | Self::Script(s)
            | Self::Protocol(s)
            | Self::Worker(s)
            | Self::Io(s) => f.write_str(s),
            Self::Timeout => f.write_str("Plugin execution exceeded its deadline"),
            Self::Cancelled => f.write_str("Plugin execution was cancelled"),
        }
    }
}
impl std::error::Error for Failure {}
impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Reply {
    pub version: u32,
    pub result: Result<Value, Failure>,
}

pub(crate) fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    reader.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Failure::Protocol(
            "Plugin worker message exceeds its byte limit".into(),
        ));
    }
    Ok(bytes)
}

/// Serve exactly one message, then return and drop the runtime before exit.
/// Call only in a helper process; `run` supplies the independent hard deadline.
pub fn serve(input: impl Read, mut output: impl Write) -> Result<(), Failure> {
    let result = (|| {
        let bytes = read_bounded(input, MAX_MESSAGE_BYTES)?;
        let request: Request = serde_json::from_slice(&bytes)
            .map_err(|e| Failure::InvalidRequest(format!("Invalid plugin request: {e}")))?;
        request.validate()?;
        crate::engine::execute(&request)
    })();
    let bytes = encode_bounded(&Reply {
        version: VERSION,
        result,
    })
    .or_else(|_| {
        encode_bounded(&Reply {
            version: VERSION,
            result: Err(Failure::Protocol(
                "Plugin result exceeds its byte limit".into(),
            )),
        })
    })?;
    output.write_all(&bytes)?;
    output.flush()?;
    Ok(())
}

pub(crate) fn encode_bounded(value: &impl Serialize) -> Result<Vec<u8>, Failure> {
    struct Buffer(Vec<u8>);
    impl Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_MESSAGE_BYTES.saturating_sub(self.0.len()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::FileTooLarge,
                    "Plugin message exceeds 32 MiB",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer(Vec::new());
    serde_json::to_writer(&mut buffer, value).map_err(|e| Failure::Protocol(e.to_string()))?;
    Ok(buffer.0)
}
