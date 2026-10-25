//! Pure-native reproducibility and adversarial qualification (no live C dependency).
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use moosicbox_opus_native::{Channels, Decoder, SampleRate};
#[cfg(feature = "celt")]
use moosicbox_opus_native::{Encoder, EncoderConfig};

const fn next(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *seed
}

#[test]
fn seeded_arbitrary_packets_never_panic_or_overrun() {
    for (rate, channels, count) in [
        (SampleRate::Hz8000, Channels::Mono, 1),
        (SampleRate::Hz12000, Channels::Stereo, 2),
        (SampleRate::Hz16000, Channels::Mono, 1),
        (SampleRate::Hz24000, Channels::Stereo, 2),
        (SampleRate::Hz48000, Channels::Stereo, 2),
    ] {
        let mut seed = 2457;
        let mut decoder = Decoder::new(rate, channels).unwrap();
        for length in 0..192 {
            let packet: Vec<u8> = (0..length)
                .map(|_| next(&mut seed).to_le_bytes()[3])
                .collect();
            eprintln!(
                "adversarial rate={rate:?} channels={channels:?} length={length} packet={packet:?}"
            );
            let mut output = vec![1234; 5760 * count + 16];
            let (audio, guard) = output.split_at_mut(5760 * count);
            if let Ok(samples) = decoder.decode(Some(&packet), audio, false) {
                assert!(samples <= 5760);
            }
            assert_eq!(guard, &[1234; 16]);
            // Malformed packet history must not make public reset fail.
            decoder.reset_state().unwrap();
        }
    }
}

#[cfg(feature = "celt")]
#[test]
fn sustained_native_celt_reconstruction_and_loss_are_deterministic() {
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz48000,
        channels: Channels::Stereo,
        bitrate: 128_000,
        frame_size: 960,
    };
    let mut first_packets = Vec::new();
    let mut first_pcm = Vec::new();
    for replay in 0..2 {
        let mut encoder = Encoder::new(config).unwrap();
        let mut decoder = Decoder::new(config.sample_rate, config.channels).unwrap();
        let mut seed = 2457;
        for frame in 0..100 {
            // Deterministic bounded integer-origin noise, not a quality surrogate.
            let input: Vec<f32> = (0..1920)
                .map(|_| {
                    let sample =
                        i16::from_le_bytes(next(&mut seed).to_le_bytes()[..2].try_into().unwrap());
                    f32::from(sample) / 131_072.0
                })
                .collect();
            let packet = encoder.encode(&input).unwrap();
            let mut pcm = vec![0; 1920];
            let samples = if frame % 17 == 8 {
                decoder.decode_loss(&mut pcm, 960).unwrap()
            } else {
                decoder.decode(Some(&packet), &mut pcm, false).unwrap()
            };
            assert_eq!(samples, 960);
            if replay == 0 {
                first_packets.push(packet);
                first_pcm.push(pcm);
            } else {
                assert_eq!(packet, first_packets[frame], "packet frame {frame}");
                assert_eq!(pcm, first_pcm[frame], "decoded frame {frame}");
            }
        }
    }
}

#[cfg(feature = "celt")]
#[test]
fn encoded_silence_stays_within_one_lsb() {
    for channels in [Channels::Mono, Channels::Stereo] {
        let count = if channels == Channels::Mono { 1 } else { 2 };
        let celt = EncoderConfig {
            channels,
            ..EncoderConfig::default()
        };
        let mut cases = vec![("celt", celt, Encoder::new(celt).unwrap())];
        cases.push(("auto", celt, Encoder::new_auto(celt).unwrap()));
        #[cfg(feature = "silk")]
        {
            let silk = EncoderConfig {
                sample_rate: SampleRate::Hz16000,
                frame_size: 320,
                ..celt
            };
            cases.push(("silk", silk, Encoder::new_silk(silk).unwrap()));
        }
        #[cfg(feature = "hybrid")]
        cases.push(("hybrid", celt, Encoder::new_hybrid(celt).unwrap()));
        for (mode, config, mut encoder) in cases {
            let mut decoder = Decoder::new(config.sample_rate, config.channels).unwrap();
            for frame in 0..12 {
                let packet = encoder
                    .encode(&vec![0.0; config.frame_size * count])
                    .unwrap();
                let mut pcm = vec![0; config.frame_size * count];
                assert_eq!(
                    decoder.decode(Some(&packet), &mut pcm, false).unwrap(),
                    config.frame_size
                );
                assert!(
                    pcm.iter().all(|&sample| i32::from(sample).abs() <= 1),
                    "{mode} {channels:?} silence frame {frame} exceeds one LSB"
                );
            }
        }
    }
}

#[test]
fn loss_duration_and_reset_are_exact_at_all_public_rates() {
    for (rate, hz) in [
        (SampleRate::Hz8000, 8000),
        (SampleRate::Hz12000, 12000),
        (SampleRate::Hz16000, 16000),
        (SampleRate::Hz24000, 24000),
        (SampleRate::Hz48000, 48000),
    ] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let count = if channels == Channels::Mono { 1 } else { 2 };
            let mut decoder = Decoder::new(rate, channels).unwrap();
            for samples in [hz / 400, hz / 200, hz / 100, hz / 50, hz / 25, hz * 3 / 50] {
                let mut output = vec![111; samples * count];
                assert_eq!(decoder.decode_loss(&mut output, samples).unwrap(), samples);
                assert!(output.iter().all(|x| *x == 0));
            }
            decoder.reset_state().unwrap();
        }
    }
}
