//! Public-constructor floor qualification; no private configuration overrides.
//! Explicit tests require coordinator approval of the public bitrate policy.
use super::*;

#[test]
#[ignore = "public floor candidate: requires guard release and pinned C oracle"]
fn public_floor_synthetic_matrix() {
    qualify_public_matrix(None);
}

#[test]
#[ignore = "public floor candidate: requires guard release, speech WAV and pinned C oracle"]
fn public_floor_real_speech_matrix() {
    let speech = completion::completion_tests::read_speech_samples();
    qualify_public_matrix(Some(&speech));
}

fn qualify_public_matrix(speech: Option<&[i16]>) {
    for rate in [8000, 12000, 16000, 24000, 48000] {
        let speech = speech.map(|input| convert_speech(input, rate));
        for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
            for milliseconds in [10, 20, 40, 60] {
                let config = EncoderConfig {
                    sample_rate: SampleRate::from_hz(rate).unwrap(),
                    channels,
                    frame_size: rate as usize * milliseconds / 1000,
                    bitrate: if channels == crate::Channels::Mono {
                        12_000
                    } else {
                        24_000
                    },
                };
                let kinds: &[&str] = if speech.is_some() {
                    &["speech", "quiet_speech"]
                } else {
                    &[
                        "silence",
                        "quiet",
                        "noise",
                        "unequal",
                        "anticorrelated",
                        "loud_quiet",
                    ]
                };
                for &kind in kinds {
                    qualify_public_case(config, kind, speech.as_deref());
                }
            }
        }
    }
}

fn convert_speech(input: &[i16], rate: u32) -> Vec<i16> {
    if rate == 8000 {
        return input.to_vec();
    }
    let mut converter = crate::silk::resampler::SilkResampler::new(8000, rate).unwrap();
    let mut output = Vec::new();
    for chunk in input.chunks(960) {
        let mut padded = chunk.to_vec();
        padded.resize(chunk.len().next_multiple_of(8), 0);
        output.extend(converter.process(&padded).unwrap());
    }
    output.extend(converter.process(&[0; 960]).unwrap());
    output
}

fn qualify_public_case(config: EncoderConfig, kind: &str, speech: Option<&[i16]>) {
    let channels = super::super::channel_count(config.channels);
    let rate = config.sample_rate as usize;
    // Include real leading silence, transitions and the complete supplied asset.
    let length = speech.map_or(config.frame_size * 4, |samples| samples.len() - 31);
    let mut random = 1_u32;
    let mut input = Vec::with_capacity(length * channels);
    for i in 0..length {
        let tone = (std::f32::consts::TAU * 143.0 * i as f32 / rate as f32).sin();
        for channel in 0..channels {
            let sample = match kind {
                "speech" | "quiet_speech" => {
                    let scale = if kind == "quiet_speech" { 0.01 } else { 1.0 };
                    f32::from(speech.unwrap()[i + channel * 31]) / 32768.0 * scale
                }
                "silence" => 0.0,
                "quiet" => tone * 0.001,
                "noise" => {
                    random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    random as i32 as f32 / 2_147_483_648.0 * 0.18
                }
                "unequal" => tone * if channel == 0 { 0.2 } else { 0.002 },
                "anticorrelated" => tone * if channel == 0 { 0.2 } else { -0.2 },
                "loud_quiet" => tone * if i < length / 2 { 0.2 } else { 0.001 },
                _ => unreachable!(),
            };
            input.push(sample);
        }
    }
    // This must fail honestly until the coordinator releases this public floor.
    let mut encoder = super::super::Encoder::new_silk(config).unwrap();
    let delay = encoder.lookahead();
    let mut packets = Vec::new();
    for chunk in input.chunks(config.frame_size * channels) {
        let mut padded = chunk.to_vec();
        padded.resize(config.frame_size * channels, 0.0);
        packets.push(encoder.encode(&padded).unwrap());
    }
    packets.extend(encoder.finish().unwrap());
    assert_eq!(encoder.finish().unwrap(), [] as [Vec<u8>; 0]);
    let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
    let mut output = Vec::new();
    for packet in &packets {
        assert!(packet.len() * rate * 8 <= config.bitrate as usize * config.frame_size);
        let mut pcm = vec![0_i16; config.frame_size * channels];
        assert_eq!(
            decoder.decode(Some(packet), &mut pcm, false).unwrap(),
            config.frame_size
        );
        output.extend(pcm);
    }
    qualification::compare_reference(config, &packets, &output);
    let aligned = &output[delay * channels..(delay + length) * channels];
    assert!(
        aligned.iter().all(|&x| x != i16::MIN && x != i16::MAX),
        "clipping {config:?} {kind}"
    );
    if kind == "silence" {
        assert!(
            output.iter().all(|&x| x.unsigned_abs() <= 1),
            "silence {config:?}"
        );
        return;
    }
    // Per-channel gates prevent a loud channel from hiding a muted quiet one.
    for channel in 0..channels {
        let mut signal = 0.0_f64;
        let mut error = 0.0_f64;
        let start = if kind == "loud_quiet" { length / 2 } else { 0 };
        for (x, y) in input[start * channels..]
            .chunks_exact(channels)
            .zip(aligned[start * channels..].chunks_exact(channels))
        {
            let x = f64::from(x[channel]);
            let difference = x - f64::from(y[channel]) / 32768.0;
            signal = x.mul_add(x, signal);
            error = difference.mul_add(difference, error);
        }
        let snr = 10.0 * (signal / error).log10();
        eprintln!("public SILK {config:?} kind={kind} channel={channel} snr={snr}");
        assert!(
            snr > 3.0,
            "public SILK {config:?} kind={kind} channel={channel} snr={snr}"
        );
    }
}

#[test]
fn standalone_low_rate_stereo_independent_gains() {
    for sign in [1.0_f32, -1.0] {
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size: 320,
            bitrate: 24_000,
        };
        let mut encoder = SilkEncoder::new(config).unwrap();
        let input: Vec<f32> = (0..config.frame_size)
            .flat_map(|i| {
                let tone = (std::f32::consts::TAU * 143.0 * i as f32 / 16000.0).sin();
                [tone * 0.2, tone * 0.198 * sign]
            })
            .collect();
        let packet = encoder.encode(&input).unwrap();
        assert!(packet.len() <= 60);
        let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
        let mut output = vec![0_i16; config.frame_size * 2];
        assert_eq!(
            decoder.decode(Some(&packet), &mut output, false).unwrap(),
            config.frame_size
        );
        let signal = input[..input.len() - 2]
            .iter()
            .map(|&x| f64::from(x).powi(2))
            .sum::<f64>();
        let error = input
            .iter()
            .zip(output.iter().skip(2))
            .map(|(&x, &y)| (f64::from(x) - f64::from(y) / 32768.0).powi(2))
            .sum::<f64>();
        let snr = 10.0 * (signal / error).log10();
        eprintln!(
            "independent gain sign={sign} indices={:?} snr={snr}",
            encoder.previous_index
        );
        assert_ne!(encoder.previous_index[0], encoder.previous_index[1]);
        assert!(snr > 3.0, "independent gain sign={sign} snr={snr}");
        assert!(output.iter().all(|&x| x != i16::MIN && x != i16::MAX));
    }
}
