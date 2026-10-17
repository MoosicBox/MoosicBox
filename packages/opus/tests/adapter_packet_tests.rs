#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::{OpusBackend, OpusDecoder};
use symphonia::core::{
    audio::Position,
    codecs::audio::{AudioCodecParameters, AudioDecoderOptions},
};
#[cfg(any(feature = "libopus", feature = "native"))]
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoder,
};

#[cfg(any(feature = "libopus", feature = "native"))]
fn samples(buffer: GenericAudioBufferRef<'_>, channel: usize) -> Vec<f32> {
    let GenericAudioBufferRef::F32(buffer) = buffer else {
        panic!("adapter must return f32")
    };
    buffer.plane(channel).unwrap().to_vec()
}

fn params(rate: u32, stereo: bool) -> AudioCodecParameters {
    let mut params = AudioCodecParameters::new();
    params.with_sample_rate(rate).with_channels(
        (if stereo {
            Position::FRONT_LEFT | Position::FRONT_RIGHT
        } else {
            Position::FRONT_LEFT
        })
        .into(),
    );
    params
}

#[cfg(any(feature = "libopus", feature = "native"))]
#[test]
fn gapless_option_controls_packet_trimming() {
    let params = params(48000, false);
    let bytes = &[0xf8, 0xff, 0xfe];
    let trimmed = trimmed_packet(0, 0, 960, 120, 240, bytes);
    for enabled in [false, true] {
        let options = AudioDecoderOptions::default().gapless(enabled);
        let mut decoder = OpusDecoder::try_new(&params, &options).unwrap();
        assert_eq!(
            decoder.decode(&trimmed).unwrap().frames(),
            if enabled { 600 } else { 960 }
        );
    }
}

#[test]
fn feature_selection_is_explicit() {
    let params = params(48000, false);
    for (backend, enabled) in [
        (OpusBackend::Libopus, cfg!(feature = "libopus")),
        (OpusBackend::Native, cfg!(feature = "native")),
    ] {
        let result = OpusDecoder::try_new_with_backend(&params, backend);
        assert_eq!(result.is_ok(), enabled);
        if let Ok(decoder) = result {
            assert_eq!(decoder.backend(), backend);
        }
    }
    let default = OpusDecoder::try_new(&params, &AudioDecoderOptions::default());
    if cfg!(feature = "libopus") {
        assert_eq!(default.unwrap().backend(), OpusBackend::Libopus);
    } else if cfg!(feature = "native") {
        assert_eq!(default.unwrap().backend(), OpusBackend::Native);
    } else {
        assert!(default.is_err());
    }
}

// A real libopus 20 ms CELT silence packet, including its TOC byte.
#[cfg(any(feature = "libopus", feature = "native"))]
const SILENCE: &[u8] = &[0xf8, 0xff, 0xfe];

#[cfg(feature = "native")]
#[test]
fn native_packet_reset_and_trimming() {
    let params = params(48000, false);
    let mut adapter = OpusDecoder::try_new_with_backend(&params, OpusBackend::Native).unwrap();
    let mut direct = moosicbox_opus_native::Decoder::new(
        moosicbox_opus_native::SampleRate::Hz48000,
        moosicbox_opus_native::Channels::Mono,
    )
    .unwrap();
    let mut expected = vec![0; 5760];
    let count = direct.decode(Some(SILENCE), &mut expected, false).unwrap();
    assert_eq!(count, 960);
    let packet = make_packet(0, 0, 960, SILENCE);
    let first = samples(adapter.decode(&packet).unwrap(), 0);
    assert_eq!(
        first,
        expected[..count]
            .iter()
            .map(|s| f32::from(*s) / 32768.0)
            .collect::<Vec<_>>()
    );
    adapter.reset();
    assert_eq!(adapter.last_decoded().frames(), 0);
    assert_eq!(samples(adapter.decode(&packet).unwrap(), 0), first);
    adapter.reset();
    let trimmed = trimmed_packet(0, 0, 960, 120, 240, SILENCE);
    assert_eq!(
        samples(adapter.decode(&trimmed).unwrap(), 0),
        &first[120..720]
    );
}

#[cfg(feature = "native")]
#[test]
fn native_multiframe_packet_is_delivered_intact() {
    // Code 1, two identical 20 ms CELT frames.
    let packet = make_packet(0, 0, 1920, &[0xf9, 0xff, 0xfe, 0xff, 0xfe]);
    let mut adapter =
        OpusDecoder::try_new_with_backend(&params(48000, false), OpusBackend::Native).unwrap();
    assert_eq!(adapter.decode(&packet).unwrap().frames(), 1920);
}

#[cfg(feature = "libopus")]
mod reference {
    use super::*;
    use audiopus::{
        Application, SampleRate,
        coder::{Decoder as ReferenceDecoder, Encoder},
    };

    fn encoded(frame_size: usize, channels: usize) -> Vec<u8> {
        let encoder = Encoder::new(
            SampleRate::Hz48000,
            if channels == 1 {
                audiopus::Channels::Mono
            } else {
                audiopus::Channels::Stereo
            },
            Application::Audio,
        )
        .unwrap();
        let input: Vec<i16> = (0..frame_size * channels)
            .map(|i| {
                if (i / channels) % 100 < 50 {
                    8000
                } else {
                    -8000
                }
            })
            .collect();
        let mut packet = vec![0; 4000];
        let count = encoder.encode(&input, &mut packet).unwrap();
        packet.truncate(count);
        packet
    }

    #[test]
    fn real_packets_match_full_packet_reference_at_all_rates() {
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for channels in [1, 2] {
                let params = params(rate, channels == 2);
                let mut adapter =
                    OpusDecoder::try_new_with_backend(&params, OpusBackend::Libopus).unwrap();
                let reference_rate = match rate {
                    8000 => SampleRate::Hz8000,
                    12000 => SampleRate::Hz12000,
                    16000 => SampleRate::Hz16000,
                    24000 => SampleRate::Hz24000,
                    _ => SampleRate::Hz48000,
                };
                let mut reference = ReferenceDecoder::new(
                    reference_rate,
                    if channels == 1 {
                        audiopus::Channels::Mono
                    } else {
                        audiopus::Channels::Stereo
                    },
                )
                .unwrap();
                for duration in [120, 240, 480, 960, 1920, 2880] {
                    let bytes = encoded(duration, channels);
                    let packet = make_packet(0, 0, 0, &bytes);
                    let mut expected = vec![0; rate as usize * 120 / 1000 * channels];
                    let count = reference
                        .decode(Some(&bytes), &mut expected, false)
                        .unwrap();
                    assert_eq!(count, duration * rate as usize / 48000);
                    let output = adapter.decode(&packet).unwrap();
                    assert_eq!(output.frames(), count);
                    for ch in 0..channels {
                        let expected: Vec<_> = (0..count)
                            .map(|i| f32::from(expected[i * channels + ch]) / 32768.0)
                            .collect();
                        assert_eq!(samples(output.clone(), ch), expected);
                    }
                }
            }
        }
    }

    #[test]
    fn reference_multiframe_reset_trim_and_error_clear() {
        // Code 3 CBR: six 20 ms frames = maximum legal 120 ms packet.
        let mut bytes = vec![0xfb, 6];
        for _ in 0..6 {
            bytes.extend_from_slice(&SILENCE[1..]);
        }
        let packet = make_packet(0, 0, 5760, &bytes);
        let mut adapter =
            OpusDecoder::try_new_with_backend(&params(48000, false), OpusBackend::Libopus).unwrap();
        let first = samples(adapter.decode(&packet).unwrap(), 0);
        assert_eq!(first.len(), 5760);
        adapter.reset();
        assert_eq!(adapter.last_decoded().frames(), 0);
        assert_eq!(samples(adapter.decode(&packet).unwrap(), 0), first);
        adapter.reset();
        let trimmed = trimmed_packet(0, 0, 5760, 312, 111, &bytes);
        assert_eq!(
            samples(adapter.decode(&trimmed).unwrap(), 0),
            &first[312..5649]
        );
        let invalid = make_packet(0, 0, 0, &[]);
        assert!(adapter.decode(&invalid).is_err());
        assert_eq!(adapter.last_decoded().frames(), 0);
    }
}

#[cfg(any(feature = "libopus", feature = "native"))]
fn make_packet(track: u32, ts: u64, dur: u64, data: &[u8]) -> symphonia::core::packet::Packet {
    symphonia::core::packet::Packet::new(
        track,
        symphonia::core::units::Timestamp::from(ts as i64),
        dur.into(),
        data,
    )
}

#[cfg(any(feature = "libopus", feature = "native"))]
fn trimmed_packet(
    track: u32,
    ts: u64,
    dur: u64,
    start: u32,
    end: u32,
    data: &[u8],
) -> symphonia::core::packet::Packet {
    let mut packet = make_packet(track, ts, dur, data);
    packet.trim_start = start.into();
    packet.trim_end = end.into();
    packet
}
