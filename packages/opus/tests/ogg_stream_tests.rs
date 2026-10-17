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
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "1",
            "-i",
            "pipe:0",
            "-c:a",
            "libvorbis",
            "-f",
            "ogg",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("independent FFmpeg required");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&vec![0; 4800 * 4])
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reader = open(output.stdout);
    assert_eq!(reader.format_info().short_name, "ogg");
    assert!(
        reader
            .default_track(symphonia::core::formats::TrackType::Audio)
            .is_some()
    );
}

#[test]
fn independent_ffmpeg_checks_generated_ogg_duration() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
            "pipe:0",
            "-map",
            "0:a:0",
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("installed ffmpeg required for independent Ogg audit");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&stream(&vec![SILENCE.to_vec(); 3], 0))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout.len() / 2,
        2457,
        "independent mono PCM frame count"
    );
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

#[cfg(feature = "libopus")]
#[test]
fn ogg_output_gain_matches_independent_reference() {
    use audiopus::{
        Application, Channels, SampleRate,
        coder::{Decoder as Reference, Encoder},
    };
    let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Audio).unwrap();
    let mut packets = Vec::new();
    for phase in 0..3 {
        let input: Vec<i16> = (0..960)
            .map(|i| {
                if (i + phase * 960) % 100 < 50 {
                    4000
                } else {
                    -4000
                }
            })
            .collect();
        let mut bytes = vec![0; 4000];
        let count = encoder.encode(&input, &mut bytes).unwrap();
        bytes.truncate(count);
        packets.push(bytes);
    }
    let mut reference = Reference::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    let mut expected = Vec::new();
    for packet in &packets {
        let mut pcm = vec![0; 5760];
        let count = reference.decode(Some(packet), &mut pcm, false).unwrap();
        assert_eq!(count, 960);
        expected.extend_from_slice(&pcm[..count]);
    }
    assert!(expected.iter().any(|sample| *sample != 0));
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
        let factor = 10.0_f32.powf(f32::from(gain) / (256.0 * 20.0));
        let expected: Vec<_> = expected[PRE_SKIP as usize..expected.len() - END_TRIM as usize]
            .iter()
            .map(|sample| (f32::from(*sample) / 32768.0) * factor)
            .collect();
        assert_pcm(&actual, &expected);
    }
}
