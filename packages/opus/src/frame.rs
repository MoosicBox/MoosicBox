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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_frame_length_all_single_byte_values() {
        // Test all single-byte encodings (0-251)
        for value in 0..=251 {
            let result = decode_frame_length(&[value]);
            assert!(result.is_ok());
            let (length, bytes_consumed) = result.unwrap();
            assert_eq!(length, value as usize);
            assert_eq!(bytes_consumed, 1);
        }
    }

    #[test]
    fn test_decode_frame_length_two_byte_boundary() {
        // Test the exact boundary at 252
        let (len, consumed) = decode_frame_length(&[252, 0]).unwrap();
        assert_eq!(len, 252);
        assert_eq!(consumed, 2);

        // Test 253
        let (len, consumed) = decode_frame_length(&[253, 0]).unwrap();
        assert_eq!(len, 253);
        assert_eq!(consumed, 2);

        // Test 254
        let (len, consumed) = decode_frame_length(&[254, 0]).unwrap();
        assert_eq!(len, 254);
        assert_eq!(consumed, 2);

        // Test 255
        let (len, consumed) = decode_frame_length(&[255, 0]).unwrap();
        assert_eq!(len, 255);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_decode_frame_length_maximum_valid() {
        // Maximum valid frame length is 1275 bytes
        // This occurs at [255, 255]: 4 * 255 + 255 = 1020 + 255 = 1275
        let result = decode_frame_length(&[255, 255]);
        assert!(result.is_ok());
        let (length, bytes_consumed) = result.unwrap();
        assert_eq!(length, 1275);
        assert_eq!(bytes_consumed, 2);
    }

    #[test]
    fn test_decode_frame_length_exceeds_maximum() {
        // Try to create a length > 1275 using formula: 4 * byte2 + byte1
        // We need to verify this triggers InvalidFrameLength
        // However, the current implementation clamps at 1275 for [255, 255]
        // so we cannot directly test this path with the current encoding scheme

        // The maximum that can be encoded is 4 * 255 + 255 = 1275
        // which is the exact maximum, so this test documents the boundary
        let result = decode_frame_length(&[255, 255]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, 1275);
    }

    #[test]
    fn test_decode_frame_length_two_byte_various_combinations() {
        // Test various two-byte combinations
        let test_cases = [
            ([252, 1], 256),    // 4 * 1 + 252 = 256
            ([252, 10], 292),   // 4 * 10 + 252 = 292
            ([253, 100], 653),  // 4 * 100 + 253 = 653
            ([254, 200], 1054), // 4 * 200 + 254 = 1054
            ([255, 254], 1271), // 4 * 254 + 255 = 1271
        ];

        for (input, expected) in test_cases {
            let (length, bytes_consumed) = decode_frame_length(&input).unwrap();
            assert_eq!(length, expected);
            assert_eq!(bytes_consumed, 2);
        }
    }
}
