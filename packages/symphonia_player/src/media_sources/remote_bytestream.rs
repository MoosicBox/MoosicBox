use std::io::{Read, Seek};

use moosicbox_stream_utils::remote_bytestream::RemoteByteStream;
use symphonia::core::io::MediaSource;

pub struct RemoteByteStreamMediaSource(RemoteByteStream);

impl From<RemoteByteStream> for RemoteByteStreamMediaSource {
    fn from(value: RemoteByteStream) -> Self {
        Self(value)
    }
}

impl Read for RemoteByteStreamMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for RemoteByteStreamMediaSource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl MediaSource for RemoteByteStreamMediaSource {
    fn is_seekable(&self) -> bool {
        log::debug!("seekable={} size={:?}", self.0.seekable, self.0.size);
        self.0.seekable && self.0.size.is_some()
    }

    fn byte_len(&self) -> Option<u64> {
        log::debug!("byte_len={:?}", self.0.size);
        self.0.size
    }
}
