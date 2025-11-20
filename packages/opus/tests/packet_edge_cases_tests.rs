#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::packet::OpusPacket;
use pretty_assertions::assert_eq;

#[test_log::test]
fn test_code_2_dtx_first_frame() {
    // Code 2 with first frame as DTX (length 0)
    let packet = vec![0x02, 0x00, 0xAA, 0xBB, 0xCC];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, Vec::<u8>::new());
    assert!(result.frames[0].is_dtx);
    assert_eq!(result.frames[1].data, vec![0xAA, 0xBB, 0xCC]);
    assert!(!result.frames[1].is_dtx);
}

#[test_log::test]
fn test_code_2_with_two_byte_frame_length() {
    // Code 2 with frame length requiring two-byte encoding (>= 252)
    let mut packet = vec![0x02, 252, 0]; // First frame length = 252
    packet.extend(vec![0xAA; 252]); // First frame data
    packet.extend(vec![0xBB, 0xCC]); // Second frame data

    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data.len(), 252);
    assert_eq!(result.frames[1].data, vec![0xBB, 0xCC]);
}

#[test_log::test]
fn test_code_3_vbr_single_frame() {
    // Code 3 VBR mode with frame_count = 1
    let packet = vec![0x03, 0x41, 0xAA, 0xBB, 0xCC];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 1);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
}

#[test_log::test]
fn test_code_3_cbr_single_frame() {
    // Code 3 CBR mode with frame_count = 1
    let packet = vec![0x03, 0x01, 0xAA, 0xBB, 0xCC];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 1);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
}

#[test_log::test]
fn test_code_3_vbr_with_dtx_frames() {
    // Code 3 VBR with DTX frames (length 0)
    let packet = vec![0x03, 0x43, 0, 2, 0xAA, 0xBB];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, Vec::<u8>::new());
    assert!(result.frames[0].is_dtx);
    assert_eq!(result.frames[1].data, vec![0xAA, 0xBB]);
    assert!(!result.frames[1].is_dtx);
    assert_eq!(result.frames[2].data, Vec::<u8>::new());
    assert!(result.frames[2].is_dtx); // Last frame has length 0, so marked as DTX
}

#[test_log::test]
fn test_code_3_max_frame_count() {
    // Code 3 with maximum frame count (48)
    let mut packet = vec![0x03, 0x30]; // frame_count = 48, CBR mode
    packet.extend(vec![0xAA; 48]); // 48 frames of 1 byte each

    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 48);
    for frame in &result.frames {
        assert_eq!(frame.data, vec![0xAA]);
    }
}

#[test_log::test]
fn test_code_3_frame_count_49_fails() {
    // Frame count > 48 should fail
    let packet = vec![0x03, 0x31]; // frame_count = 49
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_code_1_minimum_size() {
    // Code 1 with minimum valid size (2 bytes total: 1 per frame)
    let packet = vec![0x01, 0xAA, 0xBB];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA]);
    assert_eq!(result.frames[1].data, vec![0xBB]);
}

#[test_log::test]
fn test_code_2_empty_second_frame() {
    // Code 2 where second frame is empty (all remaining data consumed by first)
    let packet = vec![0x02, 3, 0xAA, 0xBB, 0xCC];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, Vec::<u8>::new());
    assert!(!result.frames[1].is_dtx); // Not explicitly DTX, just empty
}
