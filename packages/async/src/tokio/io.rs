//! I/O utilities for the Tokio runtime.
//!
//! This module re-exports Tokio's async I/O traits and utilities for reading, writing, and seeking.

pub use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
#[cfg(feature = "io")]
pub use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
