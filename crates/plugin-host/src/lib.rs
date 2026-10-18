//! Bounded JavaScript worker foundation. Not yet connected to document editing.
//!
//! The parent owns process/pipes/deadline/cancellation. QuickJS values never leave
//! the helper's execution thread. This is resource isolation, not an OS sandbox.
mod engine;
mod entry;
mod process;
mod protocol;

pub use entry::{WORKER_ARGUMENT, worker_entry};
pub use process::run;
pub use protocol::{Failure, Limits, MAX_MESSAGE_BYTES, MAX_SCRIPT_BYTES, Request, VERSION, serve};
