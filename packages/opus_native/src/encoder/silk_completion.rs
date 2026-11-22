#[cfg(test)]
pub(super) mod completion_tests {
    use super::super::*;

    #[test]
    fn practical_rate_speech_and_long_frames() {
        for rate in [8000, 12000, 16000] {
            for milliseconds in [10, 20, 40, 60] {
                for bitrate in [12000, 16000, 24000, 48000] {
                    let config = EncoderConfig {
                        sample_rate: SampleRate::from_hz(rate as u32).unwrap(),
                        channels: crate::Channels::Mono,
                        frame_size: rate * milliseconds / 1000,
                        bitrate,
                    };
                    let mut encoder = SilkEncoder::new(config).unwrap();
                    let mut decoder =
                        crate::Decoder::new(config.sample_rate, config.channels).unwrap();
                    let mut failures = 0;
                    for frame in 0..5 {
                        let input: Vec<_> = (0..config.frame_size)
                            .map(|i| {
                                let phase = std::f32::consts::TAU
                                    * 125.0
                                    * (i + frame * config.frame_size) as f32
                                    / rate as f32;
                                // Preserve the original separate multiply/add rounding.
                                let fundamental = 0.2 * phase.sin();
                                let harmonic = 0.08 * (phase * 2.0).sin();
                                fundamental + harmonic
                            })
                            .collect();
                        let packet = match encoder.encode(&input) {
                            Ok(packet) => packet,
                            Err(error) => {
                                eprintln!("{rate} {milliseconds} {bitrate} frame {frame}: {error}");
                                failures += 1;
                                continue;
                            }
                        };
                        assert!(packet.len() * 8000 <= bitrate as usize * milliseconds);
                        let mut output = vec![0_i16; config.frame_size];
                        assert_eq!(
                            decoder.decode(Some(&packet), &mut output, false).unwrap(),
                            config.frame_size
                        );
                    }
                    assert_eq!(
                        failures, 0,
                        "budget matrix {rate} Hz {milliseconds} ms {bitrate} bit/s"
                    );
                }
            }
        }
    }

    #[cfg(feature = "hybrid")]
    #[test]
    fn ranked_prefix_harmonic_history_matches_decoder() {
        for frame_size in [160, 320] {
            for bitrate in [25_600, 28_000, 46_000, 48_000, 50_000, 64_000] {
                let config = EncoderConfig {
                    sample_rate: SampleRate::Hz16000,
                    channels: crate::Channels::Stereo,
                    frame_size,
                    bitrate,
                };
                let mut encoder = SilkEncoder::new(config).unwrap();
                let mut decoder =
                    SilkDecoder::new(config.sample_rate, config.channels, (frame_size / 16) as u8)
                        .unwrap();
                let mut lowband = super::super::super::hybrid::LowBand::new(2).unwrap();
                let mut previous = [0_i16; 2];
                for frame in 0..32 {
                    let fullband: Vec<f32> = (0..frame_size * 3)
                        .flat_map(|i| {
                            (0..2).map(move |channel| {
                                let t = (frame * frame_size * 3 + i) as f32 / 48000.0;
                                (1..=8)
                                    .map(|h| {
                                        0.12 / h as f32
                                            * (std::f32::consts::TAU
                                                * (channel as f32).mul_add(37.0, 140.0)
                                                * h as f32
                                                * t)
                                                .sin()
                                    })
                                    .sum()
                            })
                        })
                        .collect();
                    let input = lowband.process(&fullband);
                    let mut range = RangeEncoder::new(1024).unwrap();
                    encoder.encode_prefix_ranked(&input, &mut range).unwrap();
                    let prefix_bits = range.tell();
                    let payload = range.finish().unwrap();
                    let mut range = crate::range::RangeDecoder::new(&payload).unwrap();
                    decoder.begin_packet();
                    decoder.decode_header_bits(&mut range, true).unwrap();
                    let mut output = vec![0_i16; frame_size * 2];
                    decoder
                        .decode_silk_frame(&mut range, true, &mut output)
                        .unwrap();
                    let mid = &encoder.pcm_history[0];
                    let side = &encoder.pcm_history[1];
                    prefix_metrics(&encoder, &input, frame, prefix_bits);
                    for i in 0..frame_size {
                        let ms = if i == 0 {
                            previous
                        } else {
                            [
                                mid[mid.len() - frame_size + i - 1],
                                side[side.len() - frame_size + i - 1],
                            ]
                        };
                        for channel in 0..2 {
                            let expected = (i32::from(ms[0])
                                + if channel == 0 {
                                    i32::from(ms[1])
                                } else {
                                    -i32::from(ms[1])
                                })
                            .clamp(-32768, 32767);
                            assert!(
                                (expected - i32::from(output[2 * i + channel])).abs() <= 2,
                                "prefix history mismatch bitrate={bitrate} frame_size={frame_size} frame={frame} sample={i} channel={channel} expected={expected} actual={} gain={:?} indices={:?}",
                                output[2 * i + channel],
                                encoder.gain,
                                encoder.previous_index
                            );
                        }
                    }
                    previous = [*mid.last().unwrap(), *side.last().unwrap()];
                }
            }
        }
    }

    #[cfg(feature = "hybrid")]
    fn prefix_metrics(encoder: &SilkEncoder, input: &[f32], frame: usize, prefix_bits: u64) {
        let frame_size = encoder.config.frame_size;
        let bitrate = encoder.config.bitrate;
        let mid = &encoder.pcm_history[0];
        let side = &encoder.pcm_history[1];
        let mut energy = [0_i128; 2];
        let mut squared_error = [0_i128; 2];
        let mut clipped = [0_usize; 2];
        for (i, pair) in input.as_chunks::<2>().0.iter().enumerate() {
            let left = (pair[0] * 32768.0).round() as i32;
            let right = (pair[1] * 32768.0).round() as i32;
            let original = [left.midpoint(right), (left - right) / 2];
            let reconstruction = [
                mid[mid.len() - frame_size + i],
                side[side.len() - frame_size + i],
            ];
            for channel in 0..2 {
                energy[channel] += i128::from(original[channel]).pow(2);
                squared_error[channel] +=
                    (i128::from(original[channel]) - i128::from(reconstruction[channel])).pow(2);
                clipped[channel] += usize::from(
                    reconstruction[channel] == i16::MIN || reconstruction[channel] == i16::MAX,
                );
            }
        }
        eprintln!(
            "SILK_PREFIX frame_size={frame_size} bitrate={bitrate} frame={frame} bits={prefix_bits} indices={:?} gains={:?} energy={energy:?} error={squared_error:?} clipped={clipped:?}",
            encoder.previous_index, encoder.gain
        );
    }

    #[cfg(feature = "hybrid")]
    #[test]
    fn ranked_prefix_music_short_frame_distortion() {
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size: 160,
            bitrate: 25_600,
        };
        let mut encoder = SilkEncoder::new(config).unwrap();
        let mut lowband = super::super::super::hybrid::LowBand::new(2).unwrap();
        let mut signal_energy = [0_i128; 2];
        let mut error_energy = [0_i128; 2];
        let mut cross = [0_i128; 2];
        let mut output_energy = [0_i128; 2];
        for frame in 0..32 {
            let fullband: Vec<f32> = (0..480)
                .flat_map(|i| {
                    let sample = frame * 480 + i;
                    let t = sample as f32 / 48000.0;
                    (0..2).map(move |channel| {
                        let tau = std::f32::consts::TAU;
                        let base = 0.16_f32.mul_add(
                            (tau * (channel as f32).mul_add(220.0, 440.0) * t).sin(),
                            0.07 * (tau * 1320.0 * t).sin(),
                        );
                        if (300..420).contains(&(sample % 960)) {
                            0.2_f32.mul_add((tau * 9000.0 * t).sin(), base)
                        } else {
                            base
                        }
                    })
                })
                .collect();
            let input = lowband.process(&fullband);
            let mut range = RangeEncoder::new(40).unwrap();
            encoder.encode_prefix_ranked(&input, &mut range).unwrap();
            prefix_metrics(&encoder, &input, frame, range.tell());
            for (i, pair) in input.as_chunks::<2>().0.iter().enumerate() {
                let start = encoder.pcm_history[0].len() - 160;
                let mid = i128::from(encoder.pcm_history[0][start + i]);
                let side = i128::from(encoder.pcm_history[1][start + i]);
                for channel in 0..2 {
                    let original = (pair[channel] * 32768.0).round() as i128;
                    let reconstructed = if channel == 0 { mid + side } else { mid - side };
                    signal_energy[channel] += original * original;
                    error_energy[channel] += (original - reconstructed).pow(2);
                    cross[channel] += original * reconstructed;
                    output_energy[channel] += reconstructed * reconstructed;
                }
            }
        }
        eprintln!(
            "SILK_MUSIC signal={signal_energy:?} error={error_energy:?} cross={cross:?} output={output_energy:?}"
        );
    }

    pub(in super::super) fn read_speech_samples() -> Vec<i16> {
        let path =
            std::env::var("MOOSICBOX_OPUS_SPEECH_WAV").expect("explicit local speech WAV required");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let mut cursor = 12;
        let mut samples = Vec::new();
        let mut format_ok = false;
        while cursor + 8 <= bytes.len() {
            let size =
                u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
            let data = &bytes[cursor + 8..cursor + 8 + size];
            match &bytes[cursor..cursor + 4] {
                b"fmt " => {
                    assert_eq!(u16::from_le_bytes(data[..2].try_into().unwrap()), 1);
                    assert_eq!(u16::from_le_bytes(data[2..4].try_into().unwrap()), 1);
                    assert_eq!(u32::from_le_bytes(data[4..8].try_into().unwrap()), 8000);
                    assert_eq!(u16::from_le_bytes(data[14..16].try_into().unwrap()), 16);
                    format_ok = true;
                }
                b"data" => samples.extend(
                    data.as_chunks::<2>()
                        .0
                        .iter()
                        .copied()
                        .map(i16::from_le_bytes),
                ),
                _ => {}
            }
            cursor += 8 + size + size % 2;
        }
        assert!(format_ok && samples.len() >= 16000);
        samples
    }

    #[test]
    #[ignore = "requires explicit local MOOSICBOX_OPUS_SPEECH_WAV signed16 mono 8k WAV"]
    fn real_speech_low_rate_budget_and_quality() {
        let samples = read_speech_samples();
        // Bound this focused first-results lane to two seconds of actual
        // speech, not the asset's leading silence. The full asset is retained
        // for the coordinator's qualification matrix and provenance hashing.
        let start = samples
            .windows(160)
            .position(|window| {
                window
                    .iter()
                    .map(|&x| i64::from(x) * i64::from(x))
                    .sum::<i64>()
                    > 160 * 10000
            })
            .expect("speech activity");
        let input = &samples[start..start + 16000];
        let mut failed_cases = Vec::new();
        for milliseconds in [10, 20, 40, 60] {
            for bitrate in [12000, 16000, 24000, 48000, 64000, 128_000] {
                let config = EncoderConfig {
                    sample_rate: SampleRate::Hz8000,
                    channels: crate::Channels::Mono,
                    frame_size: milliseconds * 8,
                    bitrate,
                };
                let mut encoder = SilkEncoder::new(config).unwrap();
                let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
                let mut reconstructed = Vec::new();
                let mut packets = Vec::new();
                let mut packet_bytes = 0;
                let mut failures = 0;
                for frame in input.chunks_exact(config.frame_size) {
                    let normalized = frame
                        .iter()
                        .map(|&x| f32::from(x) / 32768.0)
                        .collect::<Vec<_>>();
                    let Ok(packet) = encoder.encode(&normalized).inspect_err(|error| {
                        eprintln!(
                            "real speech encode failure ms={milliseconds} target={bitrate} sample={}: {error}",
                            reconstructed.len()
                        );
                    }) else {
                        failures += 1;
                        break;
                    };
                    assert!(packet.len() * 8000 <= bitrate as usize * milliseconds);
                    packet_bytes += packet.len();
                    packets.push(packet.clone());
                    let mut output = vec![0_i16; config.frame_size];
                    assert_eq!(
                        decoder.decode(Some(&packet), &mut output, false).unwrap(),
                        config.frame_size
                    );
                    reconstructed.extend(output);
                }
                if std::env::var_os("MOOSICBOX_OPUS_COMPARE_SPEECH_C").is_some() {
                    super::super::qualification::compare_reference(
                        config,
                        &packets,
                        &reconstructed,
                    );
                }
                let signal = input
                    .iter()
                    .take(reconstructed.len().saturating_sub(5))
                    .map(|&x| f64::from(x).powi(2))
                    .sum::<f64>();
                let noise = reconstructed
                    .iter()
                    .skip(5)
                    .zip(input)
                    .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
                    .sum::<f64>();
                let snr = 10.0 * (signal / noise).log10();
                let voiced = encoder.transmitted_voiced;
                let clips = reconstructed
                    .iter()
                    .filter(|&&x| x == i16::MIN || x == i16::MAX)
                    .count();
                eprintln!(
                    "real speech ms={milliseconds} target={bitrate} bytes={packet_bytes} samples={} voiced={voiced} failures={failures} snr={snr:.3} clips={clips}",
                    reconstructed.len()
                );
                if failures != 0 || voiced == 0 || snr < 5.0 || clips != 0 {
                    failed_cases.push((milliseconds, bitrate));
                }
            }
        }
        assert!(
            failed_cases.is_empty(),
            "unqualified real speech cases: {failed_cases:?}"
        );
    }

    #[test]
    fn pitch_search_only_returns_entropy_representable_lags() {
        for rate in [8000_usize, 12000, 16000] {
            let input = (0..rate / 50)
                .map(|index| {
                    if index.is_multiple_of(rate * 18 / 1000) {
                        6000
                    } else {
                        0
                    }
                })
                .collect::<Vec<_>>();
            let previous = input
                .iter()
                .map(|&sample| sample as i16)
                .collect::<Vec<_>>();
            if let Some(pitch) = prediction::analyze(&input, &previous, &[0; 10], rate) {
                let index = pitch.lag - rate / 500;
                assert!(index / (rate / 2000) < silk::LTP_LAG_HIGH_PDF.len());
                assert!(pitch.lag < rate * 18 / 1000);
            }
        }
    }

    #[test]
    fn pitch_search_rejects_silence_and_selects_periodic_speech() {
        let model = analyze(&vec![0; 320], Bandwidth::Wideband).unwrap();
        assert!(prediction::analyze(&vec![0; 320], &[], &model.lpc, 16000).is_none());
        let input: Vec<_> = (0..320)
            .map(|i| {
                let phase = std::f32::consts::TAU * i as f32 / 128.0;
                let fundamental = 6000.0 * phase.sin();
                let harmonic = 2500.0 * (2.0 * phase).sin();
                (fundamental + harmonic) as i32
            })
            .collect();
        let model = analyze(&input, Bandwidth::Wideband).unwrap();
        assert!(
            prediction::analyze(
                &input,
                &input.iter().map(|&x| x as i16).collect::<Vec<_>>(),
                &model.lpc,
                16000
            )
            .is_some()
        );
    }
}
