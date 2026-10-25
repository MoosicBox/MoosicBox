//! Public-constructor floor qualification; no private configuration overrides.
//! Explicit tests require coordinator approval of the public bitrate policy.
use super::*;

#[test]
#[ignore = "internal floor candidates only; requires reviewed media directory and pinned C oracle"]
fn internal_practical_floor_media_matrix() {
    practical_floor_media_matrix(false);
}

#[test]
#[ignore = "public constructor acceptance; requires reviewed media directory and pinned C oracle"]
fn public_practical_floor_media_matrix() {
    practical_floor_media_matrix(true);
}

fn practical_floor_media_matrix(public: bool) {
    let scope = if public { "public" } else { "internal" };
    let directory = std::env::var("MOOSICBOX_OPUS_QUALIFICATION_MEDIA").unwrap();
    let mut failures = Vec::new();
    let mut executed = 0;
    for kind in ["speech", "music"] {
        let source = reviewed_media(&directory, kind);
        for rate in [8000_u32, 12000, 16000] {
            for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
                let count = super::super::channel_count(channels);
                let converted = candidate_media(
                    &source,
                    if kind == "speech" { 8000 } else { 48000 },
                    if kind == "speech" { 1 } else { 2 },
                    rate,
                    count,
                );
                for milliseconds in [10, 20, 40, 60] {
                    for mono_bitrate in [16_000, 32_000] {
                        let config = EncoderConfig {
                            sample_rate: SampleRate::from_hz(rate).unwrap(),
                            channels,
                            frame_size: rate as usize * milliseconds / 1000,
                            bitrate: mono_bitrate * count as u32,
                        };
                        for scale in [1.0_f32, 0.01] {
                            let case = format!(
                                "{kind}/{rate}/{count}/{milliseconds}/{}/{scale}",
                                config.bitrate
                            );
                            if std::env::var("MOOSICBOX_OPUS_SILK_CASE")
                                .is_ok_and(|filter| filter != case)
                            {
                                continue;
                            }
                            executed += 1;
                            eprintln!("{scope} SILK CASE START {case}");
                            if std::panic::catch_unwind(|| {
                                let mut encoder = if public {
                                    // No private overrides: constructor rejection is a
                                    // failed public case, not an internal candidate pass.
                                    super::super::Encoder::new(config).unwrap()
                                } else {
                                    let mut encoder =
                                        super::super::Encoder::new(EncoderConfig::default())
                                            .unwrap();
                                    encoder.config = config;
                                    encoder.silk = Some(SilkEncoder::new(config).unwrap());
                                    encoder
                                };
                                let input: Vec<f32> = converted
                                    .iter()
                                    .map(|&x| f32::from(x) / 32768.0 * scale)
                                    .collect();
                                qualify_stream(config, kind, &input, &mut encoder);
                            })
                            .is_err()
                            {
                                failures.push((
                                    kind,
                                    rate,
                                    count,
                                    milliseconds,
                                    config.bitrate,
                                    scale,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(executed > 0, "case filter matched no qualification cases");
    eprintln!(
        "{scope} SILK matrix executed={executed} failures={}",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{scope} SILK matrix failures: {failures:?}"
    );
}

#[test]
#[ignore = "requires reviewed media; one actual first-frame profile and exact old/new search parity"]
fn standalone_reviewed_first_frame_search_parity() {
    let directory = std::env::var("MOOSICBOX_OPUS_QUALIFICATION_MEDIA").unwrap();
    let source = reviewed_media(&directory, "speech");
    for rate in [12000_u32, 16000] {
        let samples = candidate_media(&source, 8000, 1, rate, 1);
        for (bitrate, milliseconds, scale) in [(32000, 20, 1.0_f32), (16000, 10, 0.01)] {
            let config = EncoderConfig {
                sample_rate: SampleRate::from_hz(rate).unwrap(),
                channels: crate::Channels::Mono,
                frame_size: rate as usize * milliseconds / 1000,
                bitrate,
            };
            let input: Vec<_> = samples
                .iter()
                .take(config.frame_size)
                .map(|&sample| f32::from(sample) / 32768.0 * scale)
                .collect();
            let mut optimized = SilkEncoder::new(config).unwrap();
            let mut reference = optimized.clone();
            reference.quantizer_cache_disabled = true;
            let started = std::time::Instant::now();
            let expected = reference.encode(&input).unwrap();
            let reference_us = started.elapsed().as_micros();
            let started = std::time::Instant::now();
            let actual = optimized.encode(&input).unwrap();
            eprintln!(
                "SILK_FIRST_FRAME rate={rate} bitrate={bitrate} scale={scale} reference_us={reference_us} optimized_us={}",
                started.elapsed().as_micros()
            );
            assert_eq!(actual, expected);
            assert_eq!(optimized.history, reference.history);
            assert_eq!(optimized.gain, reference.gain);
            assert_eq!(optimized.previous_index, reference.previous_index);
            assert_eq!(optimized.pcm_history, reference.pcm_history);
        }
    }
}

fn reviewed_media(directory: &str, kind: &str) -> Vec<i16> {
    // Reuse the qualification suite's fail-closed checksum and WAV verifier.
    // Decode only verified PCM; Python's wave reader does no media conversion.
    let script = format!("{}/test_runner/src", env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .args(["-c", "import sys,wave; sys.path.insert(0,sys.argv[1]); from qualification_inputs import verify_inputs; _,p=verify_inputs(sys.argv[2]); w=wave.open(str(p[sys.argv[3]]),'rb'); sys.stdout.buffer.write(w.readframes(w.getnframes()))", &script, directory, kind])
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&x| i16::from_le_bytes(x))
        .collect()
}

fn candidate_media(
    input: &[i16],
    source_rate: u32,
    source_channels: usize,
    rate: u32,
    channels: usize,
) -> Vec<i16> {
    let mut planes = Vec::new();
    let lowband;
    let (input, source_rate) = if source_rate == 48000 {
        let mut converter = super::super::resample::SilkInput::new(source_channels, 48000).unwrap();
        let mut output = Vec::new();
        for chunk in input.chunks(960 * source_channels) {
            let mut padded: Vec<f32> = chunk.iter().map(|&x| f32::from(x) / 32768.0).collect();
            padded.resize(960 * source_channels, 0.0);
            output.extend(converter.process(&padded).unwrap());
        }
        output.extend(
            converter
                .process(&vec![0.0; 5760 * source_channels])
                .unwrap(),
        );
        lowband = output
            .iter()
            .map(|&x| (f64::from(x) * 32768.0).round().clamp(-32768.0, 32767.0) as i16)
            .collect::<Vec<_>>();
        (lowband.as_slice(), 16000)
    } else {
        (input, source_rate)
    };
    for channel in 0..channels {
        let mono: Vec<i16> = input
            .chunks_exact(source_channels)
            .map(|frame| {
                if channels == 1 && source_channels == 2 {
                    i32::from(frame[0]).midpoint(i32::from(frame[1])) as i16
                } else {
                    frame[channel % source_channels]
                }
            })
            .collect();
        let mut converter = crate::silk::resampler::SilkResampler::new(source_rate, rate).unwrap();
        let mut samples = Vec::new();
        for chunk in mono.chunks(source_rate as usize / 50) {
            let mut padded = chunk.to_vec();
            padded.resize(chunk.len().next_multiple_of(source_rate as usize / 1000), 0);
            samples.extend(converter.process(&padded).unwrap());
        }
        samples.extend(
            converter
                .process(&vec![0; source_rate as usize * 3 / 25])
                .unwrap(),
        );
        planes.push(samples);
    }
    (0..planes[0].len())
        .flat_map(|i| planes.iter().map(move |plane| plane[i]))
        .collect()
}

#[test]
#[ignore = "requires explicit pinned C oracle; verifies changed native stream without accepting a baseline"]
fn public_native_fingerprint_stream_c_parity() {
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Mono,
        bitrate: 64_000,
        frame_size: 320,
    };
    let mut encoder = super::super::Encoder::new_silk(config).unwrap();
    let mut packets = Vec::new();
    let mut state = 0x1234_5678_u32;
    let mut source_pcm = Vec::new();
    for frame in 0..100 {
        let input: Vec<f32> = (0..320)
            .map(|index| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = i32::from(state.to_le_bytes()[3]) - 128;
                let position = (frame * 320 + index) % 256;
                let triangle = position.min(256 - position) - 64;
                let integer = i16::try_from(triangle * 128 + noise * 8).unwrap();
                source_pcm.extend_from_slice(&integer.to_le_bytes());
                f32::from(integer) / 32768.0
            })
            .collect();
        packets.push(encoder.encode(&input).unwrap());
    }
    let body_packets = packets.len();
    packets.extend(encoder.finish().unwrap());
    assert_eq!(packets.len(), 101);
    let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
    let mut output = Vec::new();
    for packet in &packets {
        let mut pcm = vec![0_i16; 320];
        assert_eq!(decoder.decode(Some(packet), &mut pcm, false).unwrap(), 320);
        output.extend(pcm);
    }
    // Verify the complete stream, not a reset decoder applied only to its tail.
    qualification::compare_reference(config, &packets, &output);
    if let Some(directory) = std::env::var_os("MOOSICBOX_OPUS_SILK_PROVENANCE") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let mut wire = Vec::new();
        for packet in &packets {
            wire.extend_from_slice(&u32::try_from(packet.len()).unwrap().to_le_bytes());
            wire.extend_from_slice(packet);
        }
        std::fs::write(directory.join("silk-16000-1-100.packets"), wire).unwrap();
        std::fs::write(directory.join("silk-16000-1-100.input"), source_pcm).unwrap();
        let pcm: Vec<u8> = output
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        std::fs::write(directory.join("silk-16000-1-100.pcm"), pcm).unwrap();
    }
    eprintln!(
        "native SILK fingerprint stream C parity body_packets={body_packets} tail_packets={} samples={}",
        packets.len() - body_packets,
        output.len()
    );
}

#[test]
#[ignore = "requires explicit pinned C oracle; silence must remain within one LSB"]
fn public_silence_c_parity() {
    for sample_rate in [SampleRate::Hz8000, SampleRate::Hz12000, SampleRate::Hz16000] {
        for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
            let count = super::super::channel_count(channels);
            let config = EncoderConfig {
                sample_rate,
                channels,
                frame_size: sample_rate as usize / 50,
                bitrate: if count == 1 { 48_000 } else { 96_000 },
            };
            let mut encoder = super::super::Encoder::new_silk(config).unwrap();
            let mut decoder = crate::Decoder::new(sample_rate, channels).unwrap();
            let mut packets = Vec::new();
            let mut output = Vec::new();
            for _ in 0..12 {
                let packet = encoder
                    .encode(&vec![0.0; config.frame_size * count])
                    .unwrap();
                let mut pcm = vec![0_i16; config.frame_size * count];
                assert_eq!(
                    decoder.decode(Some(&packet), &mut pcm, false).unwrap(),
                    config.frame_size
                );
                assert!(pcm.iter().all(|sample| sample.unsigned_abs() <= 1));
                packets.push(packet);
                output.extend(pcm);
            }
            qualification::compare_reference(config, &packets, &output);
        }
    }
}

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
    qualify_stream(config, kind, &input, &mut encoder);
}

fn qualify_stream(
    config: EncoderConfig,
    kind: &str,
    input: &[f32],
    encoder: &mut super::super::Encoder,
) {
    let channels = super::super::channel_count(config.channels);
    let rate = config.sample_rate as usize;
    let length = input.len() / channels;
    let delay = encoder.lookahead();
    let mut packets = Vec::new();
    for chunk in input.chunks(config.frame_size * channels) {
        let mut padded = chunk.to_vec();
        padded.resize(config.frame_size * channels, 0.0);
        let packet = encoder.encode(&padded).unwrap();
        if let Some(silk) = &encoder.silk {
            let clipping: Vec<usize> = silk
                .pcm_history
                .iter()
                .take(channels)
                .map(|history| {
                    history
                        .iter()
                        .filter(|&&x| x == i16::MIN || x == i16::MAX)
                        .count()
                })
                .collect();
            if clipping.iter().any(|&count| count > 0) {
                let peak = padded.iter().copied().map(f32::abs).fold(0.0_f32, f32::max);
                eprintln!(
                    "SILK candidate synthesis clips frame={} config={config:?} kind={kind} input_peak={peak} gains={:?} indices={:?} clips={clipping:?}",
                    packets.len(),
                    silk.gain,
                    silk.previous_index
                );
            }
        }
        packets.push(packet);
    }
    let tail = encoder.finish().unwrap();
    assert_eq!(tail.len(), delay.div_ceil(config.frame_size));
    packets.extend(tail);
    assert_eq!(encoder.finish().unwrap(), [] as [Vec<u8>; 0]);
    let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
    let mut output = Vec::new();
    for (frame, packet) in packets.iter().enumerate() {
        assert!(packet.len() * rate * 8 <= config.bitrate as usize * config.frame_size);
        let mut pcm = vec![0_i16; config.frame_size * channels];
        assert_eq!(
            decoder.decode(Some(packet), &mut pcm, false).unwrap(),
            config.frame_size
        );
        if let Some(index) = pcm.iter().position(|&x| x == i16::MIN || x == i16::MAX) {
            eprintln!(
                "SILK decoded clip config={config:?} kind={kind} frame={frame} sample={index} value={}",
                pcm[index]
            );
        }
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
        let mut original = encoder.clone();
        pulse::REFERENCE.set(true);
        let original_packet = original.encode(&input).unwrap();
        pulse::REFERENCE.set(false);
        let packet = encoder.encode(&input).unwrap();
        assert_eq!(packet, original_packet, "stereo kernel packet attribution");
        assert_eq!(encoder.previous_index, original.previous_index);
        eprintln!(
            "stereo original kernel indices={:?}",
            original.previous_index
        );
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
        // Continue the captured failing input for ten frames: compare both
        // kernels under committed predictor history, not only a cold frame.
        for frame in 1..10 {
            pulse::REFERENCE.set(true);
            let expected = original.encode(&input).unwrap();
            pulse::REFERENCE.set(false);
            let packet = encoder.encode(&input).unwrap();
            assert_eq!(packet, expected, "stereo continuation frame={frame}");
            assert_eq!(encoder.history, original.history);
            assert_eq!(encoder.gain, original.gain);
            assert_eq!(encoder.previous_index, original.previous_index);
            assert_eq!(encoder.pcm_history, original.pcm_history);
            assert!(packet.len() <= 60);
            assert_eq!(
                decoder.decode(Some(&packet), &mut output, false).unwrap(),
                config.frame_size
            );
            assert!(output.iter().all(|&x| x != i16::MIN && x != i16::MAX));
        }
    }
}

#[test]
#[ignore = "reviewed media paired original/convex kernel packet and state parity"]
fn standalone_reviewed_pulse_kernel_parity() {
    let directory = std::env::var("MOOSICBOX_OPUS_QUALIFICATION_MEDIA").unwrap();
    let source = reviewed_media(&directory, "speech");
    for rate in [8000, 12000, 16000] {
        let samples = candidate_media(&source, 8000, 1, rate, 1);
        for (bitrate, milliseconds, scale) in [(32000, 20, 1.0_f32), (16000, 10, 0.01)] {
            let config = EncoderConfig {
                sample_rate: SampleRate::from_hz(rate).unwrap(),
                channels: crate::Channels::Mono,
                frame_size: rate as usize * milliseconds / 1000,
                bitrate,
            };
            let input: Vec<_> = samples
                .iter()
                .cycle()
                .take(config.frame_size * 2)
                .map(|&sample| f32::from(sample) / 32768.0 * scale)
                .collect();
            let mut before = Vec::new();
            let mut after = Vec::new();
            for repetition in 0..5 {
                let mut optimized = SilkEncoder::new(config).unwrap();
                let mut reference = optimized.clone();
                for (frame, input) in input.chunks_exact(config.frame_size).enumerate() {
                    let run = |encoder: &mut SilkEncoder, original| {
                        pulse::REFERENCE.set(original);
                        let started = std::time::Instant::now();
                        let packet = encoder.encode(input).unwrap();
                        (packet, started.elapsed().as_micros())
                    };
                    let (expected, actual) = if repetition % 2 == 0 {
                        let expected = run(&mut reference, true);
                        (expected, run(&mut optimized, false))
                    } else {
                        let actual = run(&mut optimized, false);
                        (run(&mut reference, true), actual)
                    };
                    pulse::REFERENCE.set(false);
                    if frame == 0 {
                        before.push(expected.1);
                        after.push(actual.1);
                    }
                    assert_eq!(actual.0, expected.0);
                    assert_eq!(optimized.history, reference.history);
                    assert_eq!(optimized.gain, reference.gain);
                    assert_eq!(optimized.previous_index, reference.previous_index);
                    assert_eq!(optimized.pcm_history, reference.pcm_history);
                }
            }
            eprintln!(
                "SILK_KERNEL_PAIR rate={rate} bitrate={bitrate} scale={scale} before_us={before:?} after_us={after:?}"
            );
        }
    }
}
