//! Opus frame structures and frame length decoding.
//!
//! This module provides types and functions for working with Opus frames within
//! packets, including frame packing modes and frame length decoding per RFC 6716.

use crate::error::{Error, Result};

/// Frame packing modes.
#[derive(Debug, Clone)]
pub enum FramePacking {
    /// Code 0: Single frame
    SingleFrame,
    /// Code 1: Two equal frames
    TwoFramesEqual,
    /// Code 2: Two variable frames
    TwoFramesVariable,
    /// Code 3: Multiple frames
    ArbitraryFrames { count: u8 },
}

/// Decode frame length from packet data.
///
/// # Errors
///
/// * `PacketTooShort` - If the data is empty or doesn't contain enough bytes for the length encoding
/// * `InvalidFrameLength` - If the decoded length exceeds the maximum of 1275 bytes
pub fn decode_frame_length(data: &[u8]) -> Result<(usize, usize)> {
    if data.is_empty() {
        return Err(Error::PacketTooShort(0));
    }

    match data[0] {
        0 => Ok((0, 1)), // DTX
        1..=251 => Ok((data[0] as usize, 1)),
        252..=255 => {
            if data.len() < 2 {
                return Err(Error::PacketTooShort(data.len()));
            }
            let length = 4 * (data[1] as usize) + (data[0] as usize);
            if length > 1275 {
                return Err(Error::InvalidFrameLength(length));
            }
            Ok((length, 2))
        }
    }
}

/// Opus frame data.
#[derive(Debug, Clone)]
pub struct OpusFrame {
    /// Frame data bytes
    pub data: Vec<u8>,
    /// Is DTX (silence) frame
    pub is_dtx: bool,
}
