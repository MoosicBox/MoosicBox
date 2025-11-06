//! Opus frame structures and frame length decoding.
//!
//! This module provides types and functions for working with Opus frames within
//! packets, including frame packing modes and frame length decoding per RFC 6716.

use crate::error::{Error, Result};

/// Frame packing modes defined by RFC 6716 Section 3.2.
///
/// The frame packing code (bits 0-1 of the TOC byte) determines how frames
/// are structured within an Opus packet.
#[derive(Debug, Clone)]
pub enum FramePacking {
    /// Code 0: Packet contains a single frame.
    ///
    /// The entire packet payload is a single Opus frame, which may be empty (DTX).
    SingleFrame,

    /// Code 1: Packet contains two frames of equal size.
    ///
    /// The payload is split evenly between two frames.
    TwoFramesEqual,

    /// Code 2: Packet contains two frames with different sizes.
    ///
    /// The first frame's size is explicitly encoded, and the second frame
    /// uses the remaining payload bytes.
    TwoFramesVariable,

    /// Code 3: Packet contains an arbitrary number of frames.
    ///
    /// Frame count and sizes are encoded in the packet header. Supports
    /// both CBR (constant bitrate) and VBR (variable bitrate) modes.
    ArbitraryFrames {
        /// Number of frames in the packet (1-48)
        count: u8,
    },
}

/// Decode frame length from packet data.
///
/// Parses the frame length encoding from RFC 6716 Section 3.2.1.
///
/// # Returns
///
/// Returns a tuple of (`frame_length`, `bytes_consumed`) where:
/// * `frame_length` - The decoded frame length in bytes (0-1275)
/// * `bytes_consumed` - Number of bytes consumed from input (1 or 2)
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

/// Opus frame data within a packet.
///
/// Represents a single encoded Opus frame, which is the fundamental unit
/// of Opus compression. Frames may represent audio data or DTX (discontinuous
/// transmission) silence frames.
#[derive(Debug, Clone)]
pub struct OpusFrame {
    /// Encoded frame data bytes.
    ///
    /// Contains the compressed audio data for this frame. Empty for DTX frames.
    pub data: Vec<u8>,

    /// Whether this is a DTX (discontinuous transmission) frame.
    ///
    /// DTX frames indicate silence and contain no audio data, allowing
    /// bandwidth savings during silent periods.
    pub is_dtx: bool,
}
