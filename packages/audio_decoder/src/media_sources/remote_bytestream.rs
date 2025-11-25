//! Remote byte stream media source for Symphonia.
//!
//! This module provides [`RemoteByteStreamMediaSource`](crate::media_sources::remote_bytestream::RemoteByteStreamMediaSource),
//! a wrapper around [`RemoteByteStream`](moosicbox_stream_utils::remote_bytestream::RemoteByteStream)
//! that implements Symphonia's `MediaSource` trait.
//!
//! This enables decoding audio from remote byte streams such as HTTP responses
//! or other network-based data sources.

use std::io::{Read, Seek};

use moosicbox_stream_utils::remote_bytestream::RemoteByteStream;
use symphonia::core::io::MediaSource;

/// A media source wrapper around [`RemoteByteStream`].
///
/// This type implements [`MediaSource`], [`Read`], and [`Seek`] to allow using
/// a remote byte stream as an audio source.
pub struct RemoteByteStreamMediaSource(RemoteByteStream);

impl From<RemoteByteStream> for RemoteByteStreamMediaSource {
    /// Converts a [`RemoteByteStream`] into a media source.
    fn from(value: RemoteByteStream) -> Self {
        Self(value)
    }
}

impl Read for RemoteByteStreamMediaSource {
    /// Reads bytes from the remote byte stream.
    ///
    /// Delegates to the underlying [`RemoteByteStream::read`] method.
    ///
    /// # Errors
    ///
    /// * Returns I/O errors from the underlying stream
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for RemoteByteStreamMediaSource {
    /// Seeks to a position in the remote byte stream.
    ///
    /// Delegates to the underlying [`RemoteByteStream::seek`] method.
    ///
    /// # Errors
    ///
    /// * Returns I/O errors from the underlying stream
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl MediaSource for RemoteByteStreamMediaSource {
    /// Returns whether this media source is seekable.
    ///
    /// A remote byte stream is seekable only if both the `seekable` flag is set
    /// and the size is known.
    fn is_seekable(&self) -> bool {
        log::debug!("seekable={} size={:?}", self.0.seekable, self.0.size);
        self.0.seekable && self.0.size.is_some()
    }

    /// Returns the total byte length of the media source.
    ///
    /// Returns the size of the remote stream, if known.
    fn byte_len(&self) -> Option<u64> {
        log::debug!("byte_len={:?}", self.0.size);
        self.0.size
    }
}
