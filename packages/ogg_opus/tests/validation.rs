use moosicbox_ogg_opus::{OggOpusReader, OggOpusWriter, OpusHead, OpusTags, ogg_crc};
use std::io::Cursor;

fn pages(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut offset = 0;
    let mut result = Vec::new();
    while offset < bytes.len() {
        let segments = usize::from(bytes[offset + 26]);
        let length = 27
            + segments
            + bytes[offset + 27..offset + 27 + segments]
                .iter()
                .map(|&n| usize::from(n))
                .sum::<usize>();
        result.push(bytes[offset..offset + length].to_vec());
        offset += length;
    }
    result
}
fn checksum(page: &mut [u8]) {
    page[22..26].fill(0);
    let crc = ogg_crc(page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
}
fn base() -> Vec<u8> {
    let mut writer = OggOpusWriter::new(
        Vec::new(),
        OpusHead::new(1, 312, 48000).unwrap(),
        &OpusTags::default(),
        73,
    )
    .unwrap();
    for _ in 0..3 {
        writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    }
    writer.finish(2457).unwrap()
}
fn read_all(bytes: Vec<u8>) -> std::io::Result<u64> {
    let mut reader = OggOpusReader::new(Cursor::new(bytes))?;
    let mut total = 0;
    while let Some(packet) = reader.next_packet()? {
        total += u64::from(packet.audible_samples());
    }
    Ok(total)
}

#[test]
fn valid_crc_does_not_hide_bad_structure() {
    let original = pages(&base());
    for (page_index, byte, value) in [
        (2, 5, 1),
        (2, 5, 2),
        (2, 14, 99),
        (2, 18, 99),
        (2, 6, 1),
        (4, 7, 255),
    ] {
        let mut changed = original.clone();
        changed[page_index][byte] = value;
        checksum(&mut changed[page_index]);
        assert!(
            read_all(changed.concat()).is_err(),
            "page {page_index} byte {byte}"
        );
    }
}

#[test]
fn nonzero_initial_granule_origin() {
    let mut all = pages(&base());
    for page in &mut all[2..] {
        let granule = u64::from_le_bytes(page[6..14].try_into().unwrap()) + 10000;
        page[6..14].copy_from_slice(&granule.to_le_bytes());
        checksum(page);
    }
    let mut reader = OggOpusReader::new(Cursor::new(all.concat())).unwrap();
    assert_eq!(reader.granule_origin(), None);
    assert_eq!(reader.total_samples().unwrap(), 2457);
    assert_eq!(reader.granule_origin(), None);
    let packet = reader.next_packet().unwrap().unwrap();
    assert_eq!(reader.granule_origin(), Some(10000));
    assert_eq!(packet.decoded_start, 10000);
    assert_eq!(packet.trim_start, 312);
    assert_eq!(reader.total_samples().unwrap(), 2457);
    assert_eq!(reader.next_packet().unwrap().unwrap().decoded_start, 10960);
    let seek = reader.seek_to(1200).unwrap();
    assert_eq!(seek.discard_until, 11512);
}

#[test]
fn duration_scan_preserves_queued_packets_and_byte_position() {
    let mut reader = OggOpusReader::new(Cursor::new(base())).unwrap();
    assert_eq!(reader.total_samples().unwrap(), 2457);
    assert_eq!(reader.next_packet().unwrap().unwrap().decoded_start, 0);
    assert_eq!(reader.total_samples().unwrap(), 2457);
    assert_eq!(reader.next_packet().unwrap().unwrap().decoded_start, 960);
    assert_eq!(
        reader.next_packet().unwrap().unwrap().audible_samples(),
        849
    );
    assert!(reader.next_packet().unwrap().is_none());
    assert_eq!(reader.total_samples().unwrap(), 2457);
    assert!(reader.next_packet().unwrap().is_none());
}

#[test]
fn multichannel_header_roundtrip_and_invalid_maps() {
    let head = OpusHead {
        channels: 6,
        pre_skip: 417,
        input_sample_rate: 44100,
        output_gain: -1792,
        mapping_family: 1,
        stream_count: 4,
        coupled_count: 2,
        channel_mapping: vec![0, 4, 1, 2, 3, 5],
    };
    assert_eq!(OpusHead::parse(&head.encode().unwrap()).unwrap(), head);
    let mut invalid = head.clone();
    invalid.channel_mapping[0] = 6;
    assert!(invalid.encode().is_err());
    invalid.channel_mapping[0] = 255;
    assert!(invalid.encode().is_ok());
}

#[test]
fn metadata_allocation_limits_and_invalid_final_length() {
    assert!(OpusTags::parse(b"OpusTags\x00\x00\x00\x00\xff\xff\xff\xff").is_err());
    let oversized = OpusTags {
        vendor: vec![0; 16 * 1024 * 1024],
        comments: Vec::new(),
    };
    assert!(
        OggOpusWriter::new(
            Vec::new(),
            OpusHead::new(1, 0, 48000).unwrap(),
            &oversized,
            1
        )
        .is_err()
    );
    let mut writer = OggOpusWriter::new(
        Vec::new(),
        OpusHead::new(1, 312, 48000).unwrap(),
        &OpusTags::default(),
        1,
    )
    .unwrap();
    writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    assert!(writer.finish(960).is_err());
}

#[test]
fn multiple_audio_packets_on_eos_page() {
    let all = pages(&base());
    let mut merged = all[2][..27].to_vec();
    merged[5] = 4;
    merged[6..14].copy_from_slice(&1700_u64.to_le_bytes());
    merged[26] = 3;
    merged.extend([3, 3, 3]);
    for _ in 0..3 {
        merged.extend([0xf8, 0xff, 0xfe]);
    }
    checksum(&mut merged);
    let bytes = [all[0].clone(), all[1].clone(), merged].concat();
    assert_eq!(read_all(bytes).unwrap(), 1388);
}
