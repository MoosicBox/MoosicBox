//! Custom media source implementations for Symphonia.
//!
//! This module provides various media source types that can be used with the audio decoder,
//! including byte stream sources, remote byte streams, and async streamable files.

/// Byte stream source implementation for streaming audio from asynchronous byte streams.
pub mod bytestream_source;
/// Remote byte stream media source wrapper.
pub mod remote_bytestream;
/// Async HTTP streaming file source with automatic chunk fetching.
pub mod streamable_file_async;
