#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::frame::decode_frame_length;
use test_case::test_case;

#[test_log::test]
fn test_frame_length_max_invalid() {
    // Frame length over 1275 should error
    let data = [252, 0]; // 4 * 0 + 252 = 252 (valid)
    assert!(decode_frame_length(&data).is_ok());

    let data = [255, 255]; // 4 * 255 + 255 = 1275 (valid max)
    let result = decode_frame_length(&data).unwrap();
    assert_eq!(result.0, 1275);
}

#[test_log::test]
fn test_frame_length_boundary_251() {
    // 251 is the last single-byte encoding
    let data = [251];
    let (length, bytes_read) = decode_frame_length(&data).unwrap();
    assert_eq!(length, 251);
    assert_eq!(bytes_read, 1);
}

#[test_log::test]
fn test_frame_length_boundary_252() {
    // 252 requires two bytes
    let data = [252, 0];
    let (length, bytes_read) = decode_frame_length(&data).unwrap();
    assert_eq!(length, 252); // 4 * 0 + 252 = 252
    assert_eq!(bytes_read, 2);
}

#[test_case(&[252, 50], 452; "two_byte_mid_range")]
#[test_case(&[253, 100], 653; "two_byte_253_base")]
#[test_case(&[254, 200], 1054; "two_byte_254_base")]
fn test_frame_length_two_byte_variations(data: &[u8], expected: usize) {
    let (length, bytes_read) = decode_frame_length(data).unwrap();
    assert_eq!(length, expected);
    assert_eq!(bytes_read, 2);
}

#[test_log::test]
fn test_frame_length_dtx_explicit() {
    // DTX (discontinuous transmission) is encoded as 0
    let data = [0];
    let (length, bytes_read) = decode_frame_length(&data).unwrap();
    assert_eq!(length, 0);
    assert_eq!(bytes_read, 1);
}

#[test_log::test]
fn test_frame_length_single_byte_mid() {
    // Test a middle value in single-byte range
    let data = [127];
    let (length, bytes_read) = decode_frame_length(&data).unwrap();
    assert_eq!(length, 127);
    assert_eq!(bytes_read, 1);
}
