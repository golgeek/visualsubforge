//! Shared, blocking resynchronization operations. Call from an owned worker.
mod identity;
mod job;
mod proposal;
mod publication;
pub use identity::FileIdentity;
pub use job::{Analysis, Request, Stage, Subtitles, analyze};
pub use proposal::{
    CueGap, CueOutcome, CueProposal, GapOutcome, Proposal, SubtitleSnapshot, UnresolvedReason,
};
pub use publication::{OutputPaths, Publication};

pub use media_analysis::Cancellation;
use std::{
    fmt,
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Cancelled,
    Timeout,
    InvalidInput(String),
    Analysis(String),
    Io(String),
    InputChanged(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Resynchronization cancelled"),
            Self::Timeout => f.write_str("Resynchronization exceeded its time limit"),
            Self::InvalidInput(s) | Self::Analysis(s) | Self::Io(s) => f.write_str(s),
            Self::InputChanged(s) => write!(f, "Input changed; run analysis again: {s}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<media_analysis::Error> for Error {
    fn from(error: media_analysis::Error) -> Self {
        match error {
            media_analysis::Error::Cancelled => Self::Cancelled,
            media_analysis::Error::Timeout => Self::Timeout,
            other => Self::Analysis(other.to_string()),
        }
    }
}

/// One deadline for the whole job, including hashing, all decode passes and fit.
pub struct Budget<'a> {
    cancellation: &'a Cancellation,
    started: Instant,
    timeout: Duration,
}
impl<'a> Budget<'a> {
    pub fn new(cancellation: &'a Cancellation, timeout: Duration) -> Self {
        Self {
            cancellation,
            started: Instant::now(),
            timeout,
        }
    }
    pub fn remaining(&self) -> Result<Duration> {
        if self.cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.timeout
            .checked_sub(self.started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(Error::Timeout)
    }
    pub fn check(&self) -> Result<()> {
        self.remaining().map(|_| ())
    }
}
