#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![cfg(feature = "native")]

use moosicbox_ogg_opus::{OggOpusWriter, OpusHead, OpusTags};
use moosicbox_opus::{OpusBackend, OpusDecoder};
use std::io::Cursor;
use symphonia::core::{
    codecs::audio::AudioDecoder,
    errors::Error,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

fn section(serial: u32, channels: u8, skip: u16, gain: i16, samples: u64) -> Vec<u8> {
    let mut head = OpusHead::new(channels, skip, 48_000).unwrap();
    head.output_gain = gain;
    let mut writer = OggOpusWriter::new(Vec::new(), head, &OpusTags::default(), serial).unwrap();
    for _ in 0..(samples + u64::from(skip)).div_ceil(960) {
        writer.write_packet(&[0xf8, 0xff, 0xfe]).unwrap();
    }
    writer.finish(samples).unwrap()
}

fn open(bytes: Vec<u8>) -> Box<dyn FormatReader> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    moosicbox_opus::create_opus_probe()
        .probe(
            &Hint::new(),
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap()
}

fn decoder(format: &dyn FormatReader) -> OpusDecoder {
    let params = format.tracks()[0]
        .codec_params
        .as_ref()
        .unwrap()
        .audio()
        .unwrap();
    OpusDecoder::try_new_with_backend(params, OpusBackend::Native).unwrap()
}

#[test]
fn nonseekable_chain_starts_without_spurious_reset_and_extracts_unread_section() {
    use std::io::Read;
    use symphonia::core::io::{MediaSource, ReadOnlySource};
    let first = section(21, 1, 312, 0, 2457);
    let second = section(22, 2, 120, 0, 1677);
    let mut bytes = first.clone();
    bytes.extend_from_slice(&second);
    let source = MediaSourceStream::new(
        Box::new(ReadOnlySource::new(Cursor::new(bytes))),
        Default::default(),
    );
    let mut format = moosicbox_opus::create_opus_probe()
        .probe(
            &Hint::new(),
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    // Three packets fill the first section. The second section must not be
    // consumed, even if its bytes were prefetched by either buffering layer.
    for _ in 0..3 {
        assert!(format.next_packet().unwrap().is_some());
    }
    let mut recovered = format.into_inner();
    assert!(!recovered.is_seekable());
    let mut unread = Vec::new();
    recovered.read_to_end(&mut unread).unwrap();
    assert_eq!(unread, second);
}

#[test]
fn chain_reconfigures_channels_gain_and_exact_audible_timing() {
    let mut bytes = section(11, 1, 312, -768, 2457);
    bytes.extend(section(12, 2, 120, 1536, 1677));
    bytes.extend(section(13, 2, 80, 0, 880));
    let mut format = open(bytes.clone());
    assert_chain_decode(&mut *format);
    use symphonia::core::io::ReadOnlySource;
    let source = MediaSourceStream::new(
        Box::new(ReadOnlySource::new(Cursor::new(bytes))),
        Default::default(),
    );
    let mut streaming = moosicbox_opus::create_opus_probe()
        .probe(
            &Hint::new(),
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    assert_chain_decode(&mut *streaming);
    format
        .seek(
            SeekMode::Accurate,
            SeekTo::Timestamp {
                ts: 2500.into(),
                track_id: 0,
            },
        )
        .unwrap();
    assert!(matches!(format.next_packet(), Err(Error::ResetRequired)));
    assert_eq!(
        format.tracks()[0]
            .codec_params
            .as_ref()
            .unwrap()
            .audio()
            .unwrap()
            .channels
            .as_ref()
            .unwrap()
            .count(),
        2
    );
    assert!(format.next_packet().unwrap().is_some());
}

fn assert_chain_decode(format: &mut dyn FormatReader) {
    let mut decoder = decoder(format);
    let mut frames = 0;
    let mut boundaries = 0;
    let mut first_after_boundary = false;
    loop {
        match format.next_packet() {
            Ok(Some(packet)) => {
                if first_after_boundary {
                    let audible =
                        packet.pts.get() + i64::try_from(packet.trim_start.get()).unwrap();
                    assert_eq!(audible, if boundaries == 1 { 2457 } else { 4134 });
                    first_after_boundary = false;
                }
                let pcm = decoder.decode_ref(&packet.as_packet_ref()).unwrap();
                frames += pcm.frames();
            }
            Ok(None) => break,
            Err(Error::ResetRequired) => {
                boundaries += 1;
                decoder = self::decoder(&*format);
                first_after_boundary = true;
            }
            Err(error) => panic!("unexpected chain error: {error}"),
        }
    }
    assert_eq!(
        boundaries, 2,
        "identical channel counts still reset prediction"
    );
    assert_eq!(frames, 5014);
}
