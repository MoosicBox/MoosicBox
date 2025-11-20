#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::packet::OpusPacket;
use pretty_assertions::assert_eq;

#[test]
fn test_code_2_first_frame_dtx_zero_length() {
    // Code 2 with first frame being DTX (length encoded as 0)
    let packet = vec![0x02, 0, 0xAA, 0xBB, 0xCC];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert!(result.frames[0].is_dtx);
    assert!(result.frames[0].data.is_empty());
    assert!(!result.frames[1].is_dtx);
    assert_eq!(result.frames[1].data, vec![0xAA, 0xBB, 0xCC]);
}

#[test]
fn test_code_2_both_frames_empty() {
    // Code 2 where first frame is DTX and second frame is empty
    let packet = vec![0x02, 0];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert!(result.frames[0].is_dtx);
    assert!(result.frames[0].data.is_empty());
    assert!(!result.frames[1].is_dtx);
    assert!(result.frames[1].data.is_empty());
}

#[test]
fn test_code_2_maximum_first_frame_length() {
    // Code 2 with maximum length first frame (1275 bytes)
    // Frame length encoding: [255, 255] = 4 * 255 + 255 = 1275
    let mut packet = vec![0x02, 255, 255];
    packet.extend(vec![0xAA; 1275]); // First frame data
    packet.extend(vec![0xBB, 0xCC]); // Second frame data

    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data.len(), 1275);
    assert_eq!(result.frames[1].data, vec![0xBB, 0xCC]);
}

#[test]
fn test_code_2_single_byte_length_encoding() {
    // Test with single byte length encoding (value < 252)
    let packet = vec![
        0x02, 100, /* frame 1 */ 0xAA, 0xBB, /* frame 2 */ 0xCC,
    ];
    let result = OpusPacket::parse(&packet);

    // This should fail because we don't have 100 bytes for frame 1
    assert!(result.is_err());
}

#[test]
fn test_code_2_two_byte_length_at_boundary() {
    // Test with two-byte length encoding at boundary (252)
    let mut packet = vec![0x02, 252, 0]; // Length = 252
    packet.extend(vec![0xAA; 252]); // First frame
    packet.push(0xBB); // Second frame

    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data.len(), 252);
    assert_eq!(result.frames[1].data, vec![0xBB]);
}

#[test]
fn test_code_2_insufficient_data_for_encoded_length() {
    // Packet declares frame length of 10 but only has 5 bytes
    let packet = vec![0x02, 10, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    let result = OpusPacket::parse(&packet);

    assert!(result.is_err());
}

#[test]
fn test_code_2_exact_data_for_first_frame_only() {
    // Packet has exactly enough data for first frame, second is empty
    let packet = vec![0x02, 3, 0xAA, 0xBB, 0xCC];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert!(result.frames[1].data.is_empty());
    assert!(!result.frames[1].is_dtx);
}
