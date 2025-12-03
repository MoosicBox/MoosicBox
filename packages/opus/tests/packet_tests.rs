#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::{frame::decode_frame_length, packet::OpusPacket, toc::TocByte};
use pretty_assertions::assert_eq;
use test_case::test_case;

#[test_case(0b00011001, 3, false, 1; "silk_nb_60ms_mono_single")]
#[test_case(0b01111101, 15, true, 1; "hybrid_fb_20ms_stereo_equal")]
#[test_case(0b10011000, 19, false, 0; "celt_wb_10ms_mono_single")]
#[test_case(0b11111111, 31, true, 3; "max_config_stereo_arbitrary")]
fn test_toc_parsing(byte: u8, config: u8, stereo: bool, code: u8) {
    let toc = TocByte::parse(byte).unwrap();
    assert_eq!(toc.config(), config);
    assert_eq!(toc.is_stereo(), stereo);
    assert_eq!(toc.frame_code(), code);
}

#[test_log::test]
fn test_packet_validation() {
    assert!(OpusPacket::parse(&[]).is_err());

    assert!(OpusPacket::parse(&[0x00]).is_ok());
}

#[test_log::test]
fn test_code_0_single_frame() {
    let packet = vec![0x00, 0x01, 0x02, 0x03];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 1);
    assert_eq!(result.frames[0].data, vec![0x01, 0x02, 0x03]);
    assert!(!result.frames[0].is_dtx);
}

#[test_log::test]
fn test_code_0_dtx_frame() {
    let packet = vec![0x00];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 1);
    assert!(result.frames[0].is_dtx);
    assert!(result.frames[0].data.is_empty());
}

#[test_log::test]
fn test_code_1_two_equal_frames() {
    let packet = vec![0x01, 0xAA, 0xBB, 0xCC, 0xDD];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.frames[1].data, vec![0xCC, 0xDD]);
    assert!(!result.frames[0].is_dtx);
    assert!(!result.frames[1].is_dtx);
}

#[test_log::test]
fn test_code_1_odd_length_fails() {
    let packet = vec![0x01, 0xAA, 0xBB, 0xCC];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_code_2_two_variable_frames() {
    let packet = vec![0x02, 2, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 2);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.frames[1].data, vec![0xCC, 0xDD, 0xEE]);
}

#[test_log::test]
fn test_code_3_cbr_mode() {
    let packet = vec![
        0x03, 0x03, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22, 0x33,
    ];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB, 0xCC]);
    assert_eq!(result.frames[1].data, vec![0xDD, 0xEE, 0xFF]);
    assert_eq!(result.frames[2].data, vec![0x11, 0x22, 0x33]);
}

#[test_log::test]
fn test_code_3_vbr_mode() {
    let packet = vec![0x03, 0x43, 2, 3, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11];
    let result = OpusPacket::parse(&packet).unwrap();

    assert_eq!(result.frames.len(), 3);
    assert_eq!(result.frames[0].data, vec![0xAA, 0xBB]);
    assert_eq!(result.frames[1].data, vec![0xCC, 0xDD, 0xEE]);
    assert_eq!(result.frames[2].data, vec![0xFF, 0x11]);
}

#[test_log::test]
fn test_code_3_invalid_frame_count_zero() {
    let packet = vec![0x03, 0x00];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_code_3_invalid_frame_count_too_high() {
    let packet = vec![0x03, 0x31];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_case(&[100], 100, 1; "single_byte")]
#[test_case(&[252, 1], 256, 2; "two_byte_min_252")]
#[test_case(&[253, 1], 257, 2; "two_byte_min_253")]
#[test_case(&[255, 255], 1275, 2; "two_byte_max")]
#[test_case(&[252, 255], 1272, 2; "two_byte_252_high")]
#[test_case(&[0], 0, 1; "dtx_zero")]
fn test_frame_length_decoding(data: &[u8], expected_len: usize, bytes_read: usize) {
    let (len, read) = decode_frame_length(data).unwrap();
    assert_eq!(len, expected_len);
    assert_eq!(read, bytes_read);
}

#[test_log::test]
fn test_frame_length_empty_fails() {
    assert!(decode_frame_length(&[]).is_err());
}

#[test_log::test]
fn test_frame_length_two_byte_truncated_fails() {
    assert!(decode_frame_length(&[252]).is_err());
}

#[test_log::test]
fn test_toc_all_configs() {
    for config in 0..=31 {
        for stereo_bit in [0, 1] {
            for frame_code in 0..=3 {
                let byte = (config << 3) | (stereo_bit << 2) | frame_code;
                let toc = TocByte::parse(byte).unwrap();

                assert_eq!(toc.config(), config);
                assert_eq!(toc.is_stereo(), stereo_bit == 1);
                assert_eq!(toc.frame_code(), frame_code);
            }
        }
    }
}

#[test_log::test]
fn test_packet_too_short_for_code_1() {
    let packet = vec![0x01, 0xAA];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_packet_too_short_for_code_2() {
    let packet = vec![0x02, 10, 0xAA];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_packet_too_short_for_code_3() {
    let packet = vec![0x03, 0x02];
    assert!(OpusPacket::parse(&packet).is_err());
}

#[test_log::test]
fn test_code_3_cbr_not_divisible() {
    let packet = vec![0x03, 0x03, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
    assert!(OpusPacket::parse(&packet).is_err());
}
