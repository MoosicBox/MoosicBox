use moosicbox_opus_native::range::RangeDecoder;

#[test]
fn test_complete_decode_sequence() {
    let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x12, 0x34, 0x56, 0x78];
    let mut decoder = RangeDecoder::new(&data).unwrap();

    let symbol = decoder.ec_decode(256).unwrap();
    decoder.ec_dec_update(symbol, symbol + 1, 256).unwrap();

    let bits = decoder.ec_dec_bits(8).unwrap();
    assert!(bits <= 255);

    let tell = decoder.ec_tell();
    assert!(tell > 0);
}

#[test]
fn test_error_recovery_empty_buffer() {
    let data = vec![];
    let result = RangeDecoder::new(&data);
    assert!(result.is_err());
}

#[test]
fn test_error_recovery_insufficient_buffer() {
    let data = vec![0xFF];
    let result = RangeDecoder::new(&data);
    assert!(result.is_err());
}

#[test]
fn test_all_public_api_functions() {
    let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x12, 0x34, 0x56, 0x78, 0x9A];
    let mut decoder = RangeDecoder::new(&data).unwrap();

    let _ = decoder.ec_decode(128).unwrap();
    let _ = decoder.ec_decode_bin(4).unwrap();

    decoder.ec_dec_update(10, 20, 128).unwrap();

    let _ = decoder.ec_dec_bit_logp(2).unwrap();

    let icdf = [252_u8, 200, 150, 100, 50, 0];
    let _ = decoder.ec_dec_icdf(&icdf, 8).unwrap();

    let _ = decoder.ec_dec_bits(4).unwrap();

    let _ = decoder.ec_dec_uint(100).unwrap();

    let _ = decoder.ec_tell();
    let _ = decoder.ec_tell_frac();
}

#[test]
fn test_sequential_symbol_decoding() {
    let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x12, 0x34, 0x56, 0x78];
    let mut decoder = RangeDecoder::new(&data).unwrap();

    for _ in 0..5 {
        let symbol = decoder.ec_decode(16).unwrap();
        assert!(symbol < 16);
        decoder.ec_dec_update(symbol, symbol + 1, 16).unwrap();
    }
}

#[test]
fn test_mixed_decode_operations() {
    let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];
    let mut decoder = RangeDecoder::new(&data).unwrap();

    let _ = decoder.ec_decode(64).unwrap();
    decoder.ec_dec_update(10, 20, 64).unwrap();

    let _ = decoder.ec_dec_bits(3).unwrap();

    let _ = decoder.ec_dec_bit_logp(1).unwrap();

    let _ = decoder.ec_dec_uint(50).unwrap();

    let tell1 = decoder.ec_tell();
    let tell_frac1 = decoder.ec_tell_frac();

    assert!(tell1 > 0);
    assert!(tell_frac1 > 0);
    assert_eq!(tell1, tell_frac1.div_ceil(8));
}
