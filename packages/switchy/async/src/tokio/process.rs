//! Process spawning and management.
//!
//! Re-exports `tokio::process` types for async process execution.

pub use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};

// Re-export std types that tokio::process uses
pub use std::process::{ExitStatus, Output, Stdio};
