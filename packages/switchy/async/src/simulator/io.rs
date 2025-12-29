//! I/O utilities for the simulator runtime.
//!
//! This module re-exports Tokio's async I/O traits and utilities. The simulator uses the same
//! I/O primitives as Tokio for compatibility.

#[cfg(feature = "io")]
pub use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
pub use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
