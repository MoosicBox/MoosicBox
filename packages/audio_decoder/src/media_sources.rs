//! Custom media source implementations for Symphonia.
//!
//! This module provides various media source types that can be used with the audio decoder,
//! including byte stream sources, remote byte streams, and async streamable files.

pub mod bytestream_source;
pub mod remote_bytestream;
pub mod streamable_file_async;
