#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![cfg(any(feature = "libopus", feature = "native"))]

use std::io::Cursor;

use moosicbox_opus::{OpusBackend, OpusDecoder, create_opus_registry};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioDecoder, AudioDecoderOptions, well_known::CODEC_ID_OPUS},
    errors::Error,
    formats::probe::Hint,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

const PRE_SKIP: u32 = 312;
const END_TRIM: u32 = 111;
const SILENCE: &[u8] = &[0xf8, 0xff, 0xfe];

// Generate genuine Ogg pages (including lacing, serial/sequence, and CRC), not
// mock format packets. One packet per page keeps granule accounting auditable.
fn page(sequence: u32, flags: u8, granule: u64, packet: &[u8]) -> Vec<u8> {
    let mut page = b"OggS\0".to_vec();
    page.push(flags);
    page.extend_from_slice(&granule.to_le_bytes());
    page.extend_from_slice(&42_u32.to_le_bytes());
    page.extend_from_slice(&sequence.to_le_bytes());
    page.extend_from_slice(&[0; 4]);
    let mut remaining = packet.len();
    let mut lacing = Vec::new();
    while remaining >= 255 {
        lacing.push(255);
        remaining -= 255;
    }
    lacing.push(u8::try_from(remaining).unwrap());
    page.push(u8::try_from(lacing.len()).unwrap());
    page.extend_from_slice(&lacing);
    page.extend_from_slice(packet);
    let mut crc = 0_u32;
    for byte in &page {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

fn stream(packets: &[Vec<u8>], gain: i16) -> Vec<u8> {
    let mut head = b"OpusHead\x01\x01".to_vec();
    head.extend_from_slice(&u16::try_from(PRE_SKIP).unwrap().to_le_bytes());
    head.extend_from_slice(&48000_u32.to_le_bytes());
    head.extend_from_slice(&gain.to_le_bytes());
    head.push(0); // Mapping family 0, mono.
    let mut bytes = page(0, 2, 0, &head);
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&0_u32.to_le_bytes()); // Empty vendor.
    tags.extend_from_slice(&0_u32.to_le_bytes()); // No comments.
    bytes.extend(page(1, 0, 0, &tags));
    for (i, packet) in packets.iter().enumerate() {
        let eos = i + 1 == packets.len();
        let granule = (i as u64 + 1) * 960 - if eos { u64::from(END_TRIM) } else { 0 };
        bytes.extend(page(
            u32::try_from(i + 2).unwrap(),
            if eos { 4 } else { 0 },
            granule,
            packet,
        ));
    }
    bytes
}

fn open(bytes: Vec<u8>) -> Box<dyn FormatReader> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("opus");
    moosicbox_opus::create_opus_probe()
        .probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap()
}

fn assert_pcm(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len(), "PCM length");
    let mismatch = actual.iter().zip(expected).position(|(a, b)| a != b);
    assert!(mismatch.is_none(), "first PCM mismatch at {mismatch:?}");
}

fn samples(buffer: GenericAudioBufferRef<'_>) -> Vec<f32> {
    let GenericAudioBufferRef::F32(buffer) = buffer else {
        panic!("expected planar f32")
    };
    buffer.plane(0).unwrap().to_vec()
}

fn decode_remaining(
    format: &mut dyn FormatReader,
    decoder: &mut dyn AudioDecoder,
) -> (Vec<f32>, Vec<(u64, u64)>) {
    let mut pcm = Vec::new();
    let mut trims = Vec::new();
    loop {
        match format.next_packet() {
            Ok(Some(packet)) => {
                trims.push((packet.trim_start.get(), packet.trim_end.get()));
                pcm.extend(samples(decoder.decode(&packet).unwrap()));
            }
            Ok(None) => break,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(error) => panic!("unexpected demux error: {error}"),
        }
    }
    (pcm, trims)
}

fn exercise_stream(backend: OpusBackend) {
    let packets = vec![SILENCE.to_vec(); 3];
    let bytes = stream(&packets, 0);
    let mut format = open(bytes.clone());
    let track = format
        .default_track(symphonia::core::formats::TrackType::Audio)
        .unwrap();
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .unwrap()
        .audio()
        .unwrap()
        .clone();
    assert_eq!(params.codec, CODEC_ID_OPUS);
    assert_eq!(track.delay, Some(PRE_SKIP));
    assert_eq!(params.sample_rate, Some(48000));
    assert_eq!(&params.extra_data.as_ref().unwrap()[..8], b"OpusHead");

    // Exercise public registry construction, not just direct adapter creation.
    let registry = create_opus_registry();
    let mut registered = registry
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .unwrap();
    let (registered_pcm, trims) = decode_remaining(&mut *format, &mut *registered);
    assert_eq!(
        registered_pcm.len(),
        2880 - trims
            .iter()
            .map(|(start, end)| (*start + *end) as usize)
            .sum::<usize>()
    );

    // Explicit selection is independent of which backend the registry defaults to.
    let mut format = open(bytes);
    let mut decoder = OpusDecoder::try_new_with_backend(&params, backend).unwrap();
    assert_eq!(decoder.backend(), backend);
    let (first, _) = decode_remaining(&mut *format, &mut decoder);
    assert_eq!(first.len(), registered_pcm.len());
    if decoder.backend()
        == OpusDecoder::try_new(&params, &AudioDecoderOptions::default())
            .unwrap()
            .backend()
    {
        assert_eq!(first, registered_pcm);
    }

    let seeked = format
        .seek(
            SeekMode::Accurate,
            SeekTo::Timestamp {
                ts: 0.into(),
                track_id,
            },
        )
        .unwrap();
    assert_eq!(seeked.actual_ts, 0.into());
    decoder.reset();
    assert_eq!(decoder.last_decoded().frames(), 0);
    let (replayed, replay_trims) = decode_remaining(&mut *format, &mut decoder);
    assert_eq!(replay_trims, trims);
    assert_pcm(&replayed, &first);
    // Keep the spec expectation, even though Symphonia 0.6.1 currently fails it.
    assert_eq!(first.len(), 2880 - (PRE_SKIP + END_TRIM) as usize);
    assert_eq!(
        trims,
        [(u64::from(PRE_SKIP), 0), (0, 0), (0, u64::from(END_TRIM))]
    );
}

#[test]
fn generic_ogg_vorbis_retains_builtin_reader() {
    // Reader selection must not depend on optional encoders in the installed
    // FFmpeg build (Homebrew's ffmpeg does not always provide libvorbis).
    let reader = open(include_bytes!("fixtures/vorbis-silence.ogg").to_vec());
    assert_eq!(reader.format_info().short_name, "ogg");
    assert!(
        reader
            .default_track(symphonia::core::formats::TrackType::Audio)
            .is_some()
    );
}

#[test]
fn independently_audited_ogg_has_exact_duration() {
    // These exact bytes were independently decoded with ffmpeg to 2457 mono
    // i16 zero samples. Compare the runtime page builder byte-for-byte, then
    // exercise the ordinary registered decoder against that frozen expectation.
    let audited = include_bytes!("fixtures/opus_trimmed_silence.ogg").to_vec();
    assert_eq!(stream(&vec![SILENCE.to_vec(); 3], 0), audited);
    let mut format = open(audited);
    let params = format.tracks()[0]
        .codec_params
        .as_ref()
        .unwrap()
        .audio()
        .unwrap()
        .clone();
    let mut decoder = create_opus_registry()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .unwrap();
    let (actual, _) = decode_remaining(&mut *format, &mut *decoder);
    assert_eq!(actual.len(), 2457);
    assert!(actual.iter().all(|sample| *sample == 0.0));
}

#[cfg(feature = "libopus")]
#[test]
fn ogg_registry_reference_duration_and_seek() {
    exercise_stream(OpusBackend::Libopus);
}

#[cfg(feature = "native")]
#[test]
fn ogg_explicit_native_duration_and_seek() {
    exercise_stream(OpusBackend::Native);
}

// Captured from the independently qualified fixed-point C reference oracle.
// SHA-256: d8538065ee6f0f48bd65377c2a660b5158f9141f2ae43261d112771f4e163986
// Encoder: 48 kHz mono, defaults; three 960-frame square-wave blocks (+/-4000,
// period 100), continuous state. PCM independently replayed from these packets
// through a fresh C decoder (48 kHz mono, gain zero). Kept exact, not a tolerance:
// system libopus 1.6.1 differs by one LSB at frame 312.
fn pinned_oracle_fixture() -> (Vec<Vec<u8>>, Vec<i16>) {
    let packets = [
        include_bytes!("fixtures/c_reference_0.packet").as_slice(),
        include_bytes!("fixtures/c_reference_1.packet").as_slice(),
        include_bytes!("fixtures/c_reference_2.packet").as_slice(),
    ]
    .into_iter()
    .map(<[u8]>::to_vec)
    .collect();
    let expected = include_bytes!("fixtures/c_reference.pcm_s16le")
        .as_chunks::<2>()
        .0
        .iter()
        .map(|sample| i16::from_le_bytes(*sample))
        .collect();
    (packets, expected)
}

#[test]
fn ogg_float_gain_matches_ungained_same_selected_backend() {
    let (packets, _) = pinned_oracle_fixture();
    let mut baseline_format = open(stream(&packets, 0));
    let params = baseline_format.tracks()[0]
        .codec_params
        .as_ref()
        .unwrap()
        .audio()
        .unwrap()
        .clone();
    let mut baseline_decoder =
        OpusDecoder::try_new(&params, &AudioDecoderOptions::default()).unwrap();
    let (baseline, _) = decode_remaining(&mut *baseline_format, &mut baseline_decoder);
    assert_eq!(baseline.len(), 2457);
    assert!(baseline.iter().any(|sample| *sample != 0.0));
    for gain in [-1536_i16, 0, 1536] {
        let mut format = open(stream(&packets, gain));
        let params = format
            .default_track(symphonia::core::formats::TrackType::Audio)
            .unwrap()
            .codec_params
            .as_ref()
            .unwrap()
            .audio()
            .unwrap()
            .clone();
        let mut decoder = create_opus_registry()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .unwrap();
        // Compare independent decoder PCM with the RFC-required container trims.
        let (actual, trims) = decode_remaining(&mut *format, &mut *decoder);
        assert_eq!(
            trims,
            [(u64::from(PRE_SKIP), 0), (0, 0), (0, u64::from(END_TRIM))]
        );
        assert_eq!(actual.len(), 2457);
        let expected: Vec<_> = baseline
            .iter()
            .map(|sample| *sample * 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0)))
            .collect();
        assert_pcm(&actual, &expected);
    }
}

#[cfg(feature = "native")]
#[test]
fn native_integer_pcm_matches_checked_in_c_reference() {
    let (packets, expected) = pinned_oracle_fixture();
    let mut decoder = moosicbox_opus_native::Decoder::new(
        moosicbox_opus_native::SampleRate::Hz48000,
        moosicbox_opus_native::Channels::Mono,
    )
    .unwrap();
    let mut actual = Vec::new();
    for packet in packets {
        let mut pcm = vec![0; 5760];
        let count = decoder.decode(Some(&packet), &mut pcm, false).unwrap();
        assert_eq!(count, 960);
        actual.extend_from_slice(&pcm[..count]);
    }
    assert_eq!(actual, expected);
    assert_eq!(
        actual[PRE_SKIP as usize..actual.len() - END_TRIM as usize].len(),
        2457
    );
}
