use moosicbox_ogg_opus::{
    ChainEvent, OggOpusChainReader, OggOpusReader, OggOpusWriter, OpusHead, OpusTags,
};
use std::io::{self, Cursor, Read};

fn section(serial: u32, channels: u8, skip: u16, samples: u64) -> Vec<u8> {
    let mut writer = OggOpusWriter::new(
        Vec::new(),
        OpusHead::new(channels, skip, 48000).unwrap(),
        &OpusTags::default(),
        serial,
    )
    .unwrap();
    let packets = (samples + u64::from(skip)).div_ceil(960).max(1);
    for _ in 0..packets {
        writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    }
    writer.finish(samples).unwrap()
}

#[test]
fn explicit_boundaries_exact_2457_and_section_timing() {
    let mut bytes = section(1, 1, 312, 2457);
    bytes.extend(section(2, 2, 120, 17));
    let mut reader = OggOpusChainReader::new(Cursor::new(bytes.clone())).unwrap();
    let mut starts = Vec::new();
    assert_eq!(reader.total_samples().unwrap(), 2474);
    let mut ends = Vec::new();
    let mut sum = 0;
    while let Some(event) = reader.next_event().unwrap() {
        match event {
            ChainEvent::SectionStart(info) => {
                starts.push((info.index, info.audible_start, info.head.channels));
            }
            ChainEvent::Packet(packet) => sum += u64::from(packet.audible_samples()),
            ChainEvent::SectionEnd { index, samples } => ends.push((index, samples)),
        }
    }
    assert_eq!(starts, [(0, 0, 1), (1, 2457, 2)]);
    assert_eq!(ends, [(0, 2457), (1, 17)]);
    assert_eq!(sum, 2474);
    assert!(reader.next_event().unwrap().is_none());
    // Legacy reader leaves the next logical section untouched.
    let mut legacy = OggOpusReader::new(Cursor::new(bytes)).unwrap();
    while legacy.next_packet().unwrap().is_some() {}
    let next = OggOpusReader::new(legacy.into_inner()).unwrap();
    assert_eq!(next.head().channels, 2);
}

#[test]
fn seek_section_preroll_and_full_section_end_duration() {
    let mut bytes = section(1, 1, 312, 2457);
    bytes.extend(section(2, 2, 120, 20000));
    bytes.extend(section(3, 1, 0, 0));
    let mut reader = OggOpusChainReader::new(Cursor::new(bytes)).unwrap();
    for sample in [0, 9000, 20000, u64::MAX] {
        let seek = reader.seek_to_section(1, sample).unwrap();
        assert_eq!(seek.section.audible_start, 2457);
        assert_eq!(seek.local.sample, sample.min(20000));
        assert!(matches!(
            reader.next_event().unwrap(),
            Some(ChainEvent::SectionStart(_))
        ));
        let mut audible = 0;
        let mut first = true;
        loop {
            match reader.next_event().unwrap().unwrap() {
                ChainEvent::Packet(packet) => {
                    if first {
                        assert!(
                            packet.decoded_start <= seek.local.discard_until.saturating_sub(3840)
                        );
                        first = false;
                    }
                    let start = (packet.decoded_start + u64::from(packet.trim_start))
                        .max(seek.local.discard_until);
                    let end = packet.decoded_start + u64::from(packet.duration - packet.trim_end);
                    audible += end.saturating_sub(start);
                }
                ChainEvent::SectionEnd { samples, .. } => {
                    assert_eq!(samples, 20000);
                    break;
                }
                _ => panic!("unexpected boundary"),
            }
        }
        assert_eq!(audible, 20000 - sample.min(20000));
        assert!(
            matches!(reader.next_event().unwrap(), Some(ChainEvent::SectionStart(info)) if info.audible_start == 22457)
        );
    }
    assert!(reader.seek_to_section(3, 0).is_err());
}

fn exhaust(bytes: Vec<u8>, limit: usize) -> io::Result<()> {
    let mut reader = OggOpusChainReader::with_section_limit(Cursor::new(bytes), limit)?;
    while reader.next_event()?.is_some() {}
    Ok(())
}

#[test]
fn reject_truncated_corrupt_reused_and_limited_chains() {
    let first = section(1, 1, 312, 2457);
    let second = section(2, 1, 0, 17);
    for length in 1..second.len() {
        let mut bytes = first.clone();
        bytes.extend(&second[..length]);
        assert!(exhaust(bytes, 2).is_err(), "prefix {length}");
    }
    let mut reused = first.clone();
    reused.extend(&first);
    assert!(exhaust(reused, 2).is_err());
    let mut bytes = first.clone();
    bytes.extend(&second);
    assert!(exhaust(bytes.clone(), 1).is_err());
    bytes[first.len() + 30] ^= 1;
    assert!(exhaust(bytes, 2).is_err());
    assert!(exhaust(Vec::new(), 2).is_err());
    assert!(exhaust(first, 0).is_err());
}

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

fn update_crc(page: &mut [u8]) {
    page[22..26].fill(0);
    let crc = moosicbox_ogg_opus::ogg_crc(page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn reject_interleaving_and_serial_change_before_eos() {
    let first = pages(&section(1, 1, 312, 2457));
    let second = pages(&section(2, 1, 0, 17));
    let mut interleaved = first[..2].concat();
    interleaved.extend(&second[0]);
    interleaved.extend(first[2..].concat());
    assert!(exhaust(interleaved, 2).is_err());
    let mut changed = first;
    changed[2][14..18].copy_from_slice(&2_u32.to_le_bytes());
    update_crc(&mut changed[2]);
    assert!(exhaust(changed.concat(), 2).is_err());
}

#[test]
fn section_seek_nonzero_granule_origin_and_nonconsuming_scan() {
    let mut second = pages(&section(2, 1, 312, 2457));
    for page in &mut second[2..] {
        let granule = u64::from_le_bytes(page[6..14].try_into().unwrap()) + 10000;
        page[6..14].copy_from_slice(&granule.to_le_bytes());
        update_crc(page);
    }
    let mut bytes = section(1, 1, 0, 17);
    bytes.extend(second.concat());
    let mut reader = OggOpusChainReader::new(Cursor::new(bytes)).unwrap();
    let seek = reader.seek_to_section(1, 1200).unwrap();
    assert_eq!(seek.local.discard_until, 11512);
    assert_eq!(reader.total_samples().unwrap(), 2474);
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::SectionStart(_))
    ));
    assert!(
        matches!(reader.next_event().unwrap(), Some(ChainEvent::Packet(packet)) if packet.decoded_start == 10000)
    );
    assert_eq!(reader.total_samples().unwrap(), 2474);
    assert!(
        matches!(reader.next_event().unwrap(), Some(ChainEvent::Packet(packet)) if packet.decoded_start == 10960)
    );
    reader.seek_to_section(0, 0).unwrap();
    assert_eq!(reader.total_samples().unwrap(), 2474);
}

#[test]
fn source_extraction_preserves_read_ahead() {
    let first = section(1, 1, 0, 17);
    let second = section(2, 1, 0, 19);
    let header_end: usize = pages(&first)[..2].iter().map(Vec::len).sum();
    let mut bytes = first;
    bytes.extend(second);
    let reader = OggOpusChainReader::new(Cursor::new(bytes.clone())).unwrap();
    assert_eq!(reader.granule_origin(), None);
    let mut buffered = reader.into_inner().unwrap();
    let mut remainder = Vec::new();
    buffered.read_to_end(&mut remainder).unwrap();
    assert_eq!(remainder, bytes[header_end..]);

    let reader = OggOpusChainReader::new(Cursor::new(bytes.clone())).unwrap();
    let mut bare = reader.try_into_inner().unwrap();
    assert_eq!(bare.position(), header_end as u64);
    remainder.clear();
    bare.read_to_end(&mut remainder).unwrap();
    assert_eq!(remainder, bytes[header_end..]);
}

#[test]
fn origin_follows_section_boundary_and_preroll_without_consuming() {
    let mut second = pages(&section(2, 1, 312, 2457));
    for page in &mut second[2..] {
        let granule = u64::from_le_bytes(page[6..14].try_into().unwrap()) + 10000;
        page[6..14].copy_from_slice(&granule.to_le_bytes());
        update_crc(page);
    }
    let mut bytes = section(1, 1, 0, 17);
    bytes.extend(second.concat());
    let mut reader = OggOpusChainReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.granule_origin(), None);
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::SectionStart(_))
    ));
    assert_eq!(reader.granule_origin(), None);
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::Packet(_))
    ));
    assert_eq!(reader.granule_origin(), Some(0));
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::SectionEnd { .. })
    ));
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::SectionStart(_))
    ));
    assert_eq!(reader.granule_origin(), None);
    reader.seek_to_section(1, 1200).unwrap();
    assert_eq!(reader.granule_origin(), Some(10000));
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::SectionStart(_))
    ));
    assert!(
        matches!(reader.next_event().unwrap(), Some(ChainEvent::Packet(packet)) if packet.decoded_start == 10000)
    );
    while reader.next_event().unwrap().is_some() {}
    let mut bare = reader.try_into_inner().unwrap();
    let mut rest = Vec::new();
    bare.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty());
}

#[test]
fn failed_section_open_remains_terminal_and_extraction_reports_unavailable() {
    let mut bytes = section(1, 1, 0, 17);
    bytes.extend_from_slice(b"OggS");
    let mut reader = OggOpusChainReader::new(Cursor::new(bytes)).unwrap();
    assert!(reader.next_event().unwrap().is_some());
    assert!(reader.next_event().unwrap().is_some());
    assert!(matches!(
        reader.next_event().unwrap(),
        Some(ChainEvent::SectionEnd { .. })
    ));
    assert!(reader.next_event().is_err());
    assert_eq!(reader.granule_origin(), None);
    assert_eq!(
        reader.next_event().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.try_into_inner().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn failed_seek_remains_terminal_and_extraction_reports_unavailable() {
    let mut reader = OggOpusChainReader::new(Cursor::new(section(1, 1, 0, 17))).unwrap();
    assert_eq!(
        reader.seek_to_section(1, 0).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.next_event().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.total_samples().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(reader.into_inner().is_none());
}

struct ShortRead(Cursor<Vec<u8>>);
impl Read for ShortRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let len = buffer.len().min(1);
        self.0.read(&mut buffer[..len])
    }
}

#[test]
fn one_byte_reads_and_continued_headers() {
    let tags = OpusTags {
        vendor: vec![b'v'; 70000],
        comments: vec![],
    };
    let mut writer =
        OggOpusWriter::new(Vec::new(), OpusHead::new(1, 0, 48000).unwrap(), &tags, 2).unwrap();
    writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    let mut bytes = section(1, 1, 0, 0);
    bytes.extend(writer.finish(17).unwrap());
    let mut reader = OggOpusChainReader::new(ShortRead(Cursor::new(bytes))).unwrap();
    let mut ends = 0;
    while let Some(event) = reader.next_event().unwrap() {
        match event {
            ChainEvent::SectionStart(info) if info.index == 1 => assert_eq!(info.tags, tags),
            ChainEvent::SectionEnd { .. } => ends += 1,
            _ => {}
        }
    }
    assert_eq!(ends, 2);
}
