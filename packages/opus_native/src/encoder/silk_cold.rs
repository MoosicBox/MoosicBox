//! Exact captured cold-entry prefix qualification (test-only).
use super::*;

pub(super) fn trace_trial(
    (trial, pressure, bits, budget): (i32, u32, u64, usize),
    indices: &[i32],
    inputs: &[Vec<i32>],
    quantized: &[SharedQuantizedFrame],
) {
    let error: i128 = inputs
        .iter()
        .zip(quantized)
        .map(|(input, frame)| {
            let pcm = &frame.2;
            input
                .iter()
                .zip(pcm)
                .map(|(&target, &sample)| (i128::from(target) - i128::from(sample)).pow(2))
                .sum::<i128>()
        })
        .sum();
    eprintln!(
        "SILK_TRIAL trial={trial} pressure={pressure} bits={bits} budget={budget} fit={} error={error} indices={indices:?}",
        bits as usize <= budget
    );
}

#[test]
#[cfg(feature = "hybrid")]
#[ignore = "requires actual snapshot via MOOSICBOX_OPUS_SILK_PREFIX_REPLAY; enable SILK_TRIAL_TRACE for fit frontier"]
fn captured_prefix_budget_frontier() {
    let path = std::env::var_os("MOOSICBOX_OPUS_SILK_PREFIX_REPLAY").unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    let mut words = text
        .split_whitespace()
        .map(|word| word.parse::<i64>().unwrap());
    let bitrate = words.next().unwrap() as u32;
    let frame_size = words.next().unwrap() as usize;
    let channels = match words.next().unwrap() {
        1 => crate::Channels::Mono,
        2 => crate::Channels::Stereo,
        _ => panic!("invalid snapshot channels"),
    };
    let capacity = words.next().unwrap() as usize;
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels,
        frame_size,
        bitrate,
    };
    let mut state = SilkEncoder::new(config).unwrap();
    for channel in 0..2 {
        state.gain[channel] = words.next().unwrap() as i32;
        state.previous_index[channel] = words.next().unwrap() as i32;
        for value in &mut state.history[channel] {
            *value = words.next().unwrap() as i32;
        }
        let count = words.next().unwrap() as usize;
        state.pcm_history[channel] = words
            .by_ref()
            .take(count)
            .map(|value| value as i16)
            .collect();
    }
    let input: Vec<_> = words.map(|value| f32::from_bits(value as u32)).collect();
    assert_eq!(
        input.len(),
        frame_size * super::super::channel_count(channels)
    );
    let mut reference = state.clone();
    reference.quantizer_cache_disabled = true;
    let mut coder = RangeEncoder::new(capacity).unwrap();
    let mut reference_coder = coder.clone();
    reference
        .encode_prefix_ranked(&input, &mut reference_coder)
        .unwrap();
    state.encode_prefix_ranked(&input, &mut coder).unwrap();
    assert_eq!(format!("{coder:?}"), format!("{reference_coder:?}"));
    assert_eq!(state.history, reference.history);
    assert_eq!(state.gain, reference.gain);
    assert_eq!(state.previous_index, reference.previous_index);
    assert_eq!(state.pcm_history, reference.pcm_history);
    assert_eq!(
        coder.clone().finish().unwrap(),
        reference_coder.finish().unwrap()
    );
    eprintln!(
        "SILK_REPLAY bitrate={bitrate} frame_size={frame_size} capacity={capacity} bits={} indices={:?}",
        coder.tell(),
        state.previous_index
    );
    coder.finish().unwrap();
}

/// Opt-in snapshots of actual prefix calls, including committed predictor state.
/// Only pristine incoming coders can currently be replayed by this diagnostic.
#[cfg(feature = "hybrid")]
pub(super) fn capture_prefix(state: &SilkEncoder, input: &[f32], coder: &RangeEncoder) {
    static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let Some(directory) = std::env::var_os("MOOSICBOX_OPUS_SILK_PREFIX_CAPTURE") else {
        return;
    };
    if coder.tell() != 1 || state.config.sample_rate != SampleRate::Hz16000 {
        return;
    }
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut words = vec![
        state.config.bitrate.to_string(),
        state.config.frame_size.to_string(),
        super::super::channel_count(state.config.channels).to_string(),
        coder.clone().finish().unwrap().len().to_string(),
    ];
    for channel in 0..2 {
        words.push(state.gain[channel].to_string());
        words.push(state.previous_index[channel].to_string());
        words.extend(state.history[channel].iter().map(ToString::to_string));
        words.push(state.pcm_history[channel].len().to_string());
        words.extend(state.pcm_history[channel].iter().map(ToString::to_string));
    }
    words.extend(input.iter().map(|value| value.to_bits().to_string()));
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir_all(directory).unwrap();
    let path = directory.join(format!("prefix-{sequence}.txt"));
    std::fs::write(&path, words.join("\n")).unwrap();
    eprintln!(
        "SILK prefix snapshot {} bitrate={}",
        path.display(),
        state.config.bitrate
    );
}

#[test]
fn quantizer_cache_uses_exact_channel_and_spectral_identity() {
    let mut cache = QuantizerCache::default();
    let first = cache.model_id(0, &[17, -29], None);
    assert_eq!(first, cache.model_id(0, &[17, -29], None));
    assert_ne!(first, cache.model_id(1, &[17, -29], None));
    assert_ne!(first, cache.model_id(0, &[17, -28], None));
    assert_ne!(first, cache.model_id(0, &[17, -29, 0], None));
    assert_eq!(cache.models.len(), 4);
}

#[test]
#[cfg(feature = "hybrid")]
fn memoized_low_budget_prefix_is_bit_exact() {
    for bitrate in [25_600, 48_000] {
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size: 320,
            bitrate,
        };
        let mut cached = SilkEncoder::new(config).unwrap();
        let mut uncached = cached.clone();
        uncached.quantizer_cache_disabled = true;
        let mut seed = 1_u32;
        let mut cached_decoder = SilkDecoder::new(config.sample_rate, config.channels, 20).unwrap();
        let mut reference_decoder = cached_decoder.clone();
        for _ in 0..3 {
            let input: Vec<_> = (0..640)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let sample = i16::from_le_bytes(seed.to_le_bytes()[..2].try_into().unwrap());
                    f32::from(sample) / 131_072.0
                })
                .collect();
            let capacity = bitrate as usize / 400;
            let mut actual = RangeEncoder::new(capacity).unwrap();
            let mut expected = actual.clone();
            cached.encode_prefix_ranked(&input, &mut actual).unwrap();
            uncached
                .encode_prefix_ranked(&input, &mut expected)
                .unwrap();
            assert_eq!(actual.tell(), expected.tell());
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            let packet = actual.finish().unwrap();
            assert_eq!(packet, expected.finish().unwrap());
            let mut outputs = Vec::new();
            for decoder in [&mut cached_decoder, &mut reference_decoder] {
                let mut range = crate::range::RangeDecoder::new(&packet).unwrap();
                decoder.begin_packet();
                decoder.decode_header_bits(&mut range, true).unwrap();
                let mut output = vec![0_i16; 640];
                decoder
                    .decode_silk_frame(&mut range, true, &mut output)
                    .unwrap();
                outputs.push(output);
            }
            assert_eq!(outputs[0], outputs[1]);
            assert_eq!(cached.history, uncached.history);
            assert_eq!(cached.gain, uncached.gain);
            assert_eq!(cached.previous_index, uncached.previous_index);
            assert_eq!(cached.pcm_history, uncached.pcm_history);
        }
        let (calls, hits) = cached.quantizer_cache_counts;
        eprintln!("SILK memo bitrate={bitrate} quantizer_calls={calls} cache_hits={hits}");
        assert!(hits > 0);
        assert_eq!(calls + hits, uncached.quantizer_cache_counts.0);
    }
}

#[test]
#[ignore = "requires actual consecutive adaptive_low captures via MOOSICBOX_OPUS_COLD_SEQUENCE"]
fn captured_cold_prefix_sequence() {
    // Comma-separated paths in actual stream order, starting at SILK reset.
    // Every capture must have the producer's .meta sidecar. No inferred paths,
    // regenerated tones, or state resets between constituent packets.
    let paths = std::env::var("MOOSICBOX_OPUS_COLD_SEQUENCE").unwrap();
    let mut encoder = None;
    let mut decoder = None;
    let mut previous_ms = [0_i16; 2];
    let mut captured_decoder = None;
    let mut failures = Vec::new();
    for (ordinal, path) in paths.split(',').enumerate() {
        let metadata = std::fs::read_to_string(format!("{path}.meta")).unwrap();
        let field = |name: &str| {
            metadata
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{name}=")))
                .unwrap()
        };
        assert_eq!(field("sample_rate"), "16000");
        assert_eq!(field("channels"), "2");
        assert_eq!(
            field("silk_reset"),
            if ordinal == 0 { "true" } else { "false" }
        );
        let frame_size = field("samples_per_channel").parse::<usize>().unwrap();
        let bitrate = field("prefix_bitrate").parse::<u32>().unwrap();
        let capacity = field("packet_payload_bytes").parse::<usize>().unwrap();
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size,
            bitrate,
        };
        let encoder = encoder.get_or_insert_with(|| SilkEncoder::new(config).unwrap());
        assert_eq!(encoder.config.frame_size, frame_size);
        encoder.set_bitrate(bitrate).unwrap();
        let decoder = decoder.get_or_insert_with(|| {
            SilkDecoder::new(config.sample_rate, config.channels, (frame_size / 16) as u8).unwrap()
        });
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes.len(), frame_size * 8);
        let input: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&x| f32::from_le_bytes(x))
            .collect();
        let old_indices = encoder.previous_index;
        let mut range = RangeEncoder::new(capacity).unwrap();
        assert_eq!(range.tell().to_string(), field("incoming_range_tell"));
        if std::env::var_os("MOOSICBOX_OPUS_COLD_FRONTIER").is_some() {
            budget_frontier(encoder, &input, capacity);
        }
        let diagnostic_state = encoder.clone();
        encoder.encode_prefix_ranked(&input, &mut range).unwrap();
        if std::env::var_os("MOOSICBOX_OPUS_COLD_MODELS").is_some() {
            model_diagnostics(&diagnostic_state, encoder, &input);
        }
        let bits = range.tell();
        let payload = range.finish().unwrap();
        let mut range = crate::range::RangeDecoder::new(&payload).unwrap();
        decoder.begin_packet();
        decoder.decode_header_bits(&mut range, true).unwrap();
        let mut output = vec![0_i16; frame_size * 2];
        decoder
            .decode_silk_frame(&mut range, true, &mut output)
            .unwrap();
        let (maximum, snr) = sequence_metrics(encoder, &input, &output, previous_ms);
        previous_ms = [
            *encoder.pcm_history[0].last().unwrap(),
            *encoder.pcm_history[1].last().unwrap(),
        ];
        eprintln!(
            "cold sequence frame={} bits={bits} old_indices={old_indices:?} selected={:?} gains={:?} synthesis_max={maximum} snr={snr}",
            field("source_frame"),
            encoder.previous_index,
            encoder.gain
        );
        let (captured_max, captured_snr) =
            captured_metrics(path, config, &input, &output, &mut captured_decoder);
        // Old captured packets document the failing baseline. After a source
        // correction their output must no longer be required to equal replay.
        // Keep exact equality available for baseline/provenance reproduction.
        let require_captured_match = std::env::var_os("MOOSICBOX_OPUS_COLD_BASELINE").is_some();
        if maximum > 2
            || snr <= 3.0
            || !snr.is_finite()
            || (require_captured_match && captured_max > 2)
        {
            failures.push((path.to_owned(), maximum, snr, captured_max, captured_snr));
        }
    }
    assert!(
        failures.is_empty(),
        "persistent prefix failures: {failures:?}"
    );
}

fn sequence_metrics(
    encoder: &SilkEncoder,
    input: &[f32],
    output: &[i16],
    previous_ms: [i16; 2],
) -> (i32, f64) {
    let frame_size = input.len() / 2;
    let mut maximum = 0_i32;
    let mut signal = 0.0_f64;
    let mut error = 0.0_f64;
    for i in 0..frame_size {
        let ms = if i == 0 {
            previous_ms
        } else {
            [
                encoder.pcm_history[0][encoder.pcm_history[0].len() - frame_size + i - 1],
                encoder.pcm_history[1][encoder.pcm_history[1].len() - frame_size + i - 1],
            ]
        };
        let expected = [
            (i32::from(ms[0]) + i32::from(ms[1])).clamp(-32768, 32767),
            (i32::from(ms[0]) - i32::from(ms[1])).clamp(-32768, 32767),
        ];
        for channel in 0..2 {
            maximum = maximum.max((expected[channel] - i32::from(output[i * 2 + channel])).abs());
            if i > 0 {
                let x = f64::from(input[(i - 1) * 2 + channel]);
                let difference = x - f64::from(output[i * 2 + channel]) / 32768.0;
                signal = x.mul_add(x, signal);
                error = difference.mul_add(difference, error);
            }
        }
    }
    let snr = 10.0 * (signal / error).log10();
    (maximum, snr)
}

fn captured_metrics(
    path: &str,
    config: EncoderConfig,
    input: &[f32],
    output: &[i16],
    captured_decoder: &mut Option<SilkDecoder>,
) -> (i32, f64) {
    let frame_size = config.frame_size;
    let packet = std::fs::read(format!("{path}.packet")).unwrap();

    assert_eq!(packet[0] & 3, 0, "capture requires single-frame packet");
    let captured_decoder = captured_decoder.get_or_insert_with(|| {
        SilkDecoder::new(config.sample_rate, config.channels, (frame_size / 16) as u8).unwrap()
    });
    let mut captured_range = crate::range::RangeDecoder::new(&packet[1..]).unwrap();
    captured_decoder.begin_packet();
    captured_decoder
        .decode_header_bits(&mut captured_range, true)
        .unwrap();
    let mut captured_output = vec![0_i16; frame_size * 2];
    captured_decoder
        .decode_silk_frame(&mut captured_range, true, &mut captured_output)
        .unwrap();
    let captured_max = output
        .iter()
        .zip(&captured_output)
        .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
        .max()
        .unwrap();
    let mut captured_signal = 0.0_f64;
    let mut captured_error = 0.0_f64;
    for (&x, &y) in input.iter().zip(captured_output.iter().skip(2)) {
        let x = f64::from(x);
        let difference = x - f64::from(y) / 32768.0;
        captured_signal = x.mul_add(x, captured_signal);
        captured_error = difference.mul_add(difference, captured_error);
    }
    let captured_snr = 10.0 * (captured_signal / captured_error).log10();
    eprintln!(
        "captured packet path={path} captured_vs_replay_max={captured_max} captured_snr={captured_snr}"
    );
    (captured_max, captured_snr)
}

#[test]
fn standalone_public_silence_one_lsb() {
    for rate in [SampleRate::Hz8000, SampleRate::Hz12000, SampleRate::Hz16000] {
        for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
            let count = super::super::channel_count(channels);
            let config = EncoderConfig {
                sample_rate: rate,
                channels,
                frame_size: rate as usize / 50,
                bitrate: if count == 1 { 48_000 } else { 96_000 },
            };
            let mut encoder = super::super::Encoder::new_silk(config).unwrap();
            let mut decoder = crate::Decoder::new(rate, channels).unwrap();
            for frame in 0..12 {
                let packet = encoder
                    .encode(&vec![0.0; config.frame_size * count])
                    .unwrap();
                let mut output = vec![0_i16; config.frame_size * count];
                assert_eq!(
                    decoder.decode(Some(&packet), &mut output, false).unwrap(),
                    config.frame_size
                );
                let maximum = output.iter().map(|x| x.unsigned_abs()).max().unwrap();
                assert!(
                    maximum <= 1,
                    "silence {config:?} frame={frame} maximum={maximum}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires captured adaptive_low f32 files"]
fn captured_cold_prefix_quality() {
    for (path, frame_size, bitrate, capacity) in [
        ("/tmp/opus-cold-entry-1-960-16.f32", 320, 48_000, 159),
        ("/tmp/opus-cold-entry-3-480-4.f32", 160, 25_600, 39),
    ] {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes.len(), frame_size * 2 * 4);
        let input: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&x| f32::from_le_bytes(x))
            .collect();
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size,
            bitrate,
        };
        let mut encoder = SilkEncoder::new(config).unwrap();
        let mut range = RangeEncoder::new(capacity).unwrap();
        assert_eq!(range.tell(), 1);
        encoder.encode_prefix_ranked(&input, &mut range).unwrap();
        let bits = range.tell();
        let payload = range.finish().unwrap();
        let mut range = crate::range::RangeDecoder::new(&payload).unwrap();
        let mut decoder =
            SilkDecoder::new(config.sample_rate, config.channels, (frame_size / 16) as u8).unwrap();
        decoder.begin_packet();
        decoder.decode_header_bits(&mut range, true).unwrap();
        let mut output = vec![0_i16; frame_size * 2];
        assert_eq!(
            decoder
                .decode_silk_frame(&mut range, true, &mut output)
                .unwrap(),
            frame_size
        );
        let mut signal = 0.0_f64;
        let mut error = 0.0_f64;
        // Same-rate SILK includes stereo's one sample plus resampler delay.
        for (original, reconstructed) in input
            .as_chunks::<2>()
            .0
            .iter()
            .zip(output.as_chunks::<2>().0.iter().skip(1))
        {
            for channel in 0..2 {
                let x = f64::from(original[channel]);
                let y = f64::from(reconstructed[channel]) / 32768.0;
                signal = x.mul_add(x, signal);
                error += (x - y).powi(2);
            }
        }
        eprintln!(
            "cold prefix path={path} bits={bits} gains={:?} indices={:?} snr={}",
            encoder.gain,
            encoder.previous_index,
            10.0 * (signal / error).log10()
        );
    }
}

// Counterfactual prefix budgets only. Never changes production allocation or
// commits any candidate back to the persistent replay encoder.
fn budget_frontier(encoder: &SilkEncoder, input: &[f32], capacity: usize) {
    for budget in [31_usize, 33, 35] {
        let mut candidate = encoder.clone();
        let bitrate = ((budget + 1) * 16000 * 8 / candidate.config.frame_size) as u32;
        candidate.set_bitrate(bitrate).unwrap();
        let mut range = RangeEncoder::new(capacity).unwrap();
        candidate.encode_prefix_ranked(input, &mut range).unwrap();
        let mut signal = 0_i128;
        let mut error = 0_i128;
        let frame_size = candidate.config.frame_size;
        for (i, frame) in input.as_chunks::<2>().0.iter().enumerate() {
            let left = (f64::from(frame[0]) * 32768.0).round() as i32;
            let right = (f64::from(frame[1]) * 32768.0).round() as i32;
            for (channel, target) in [left.midpoint(right), (left - right) / 2]
                .into_iter()
                .enumerate()
            {
                let pcm = &candidate.pcm_history[channel];
                let reconstructed = i128::from(pcm[pcm.len() - frame_size + i]);
                signal += i128::from(target).pow(2);
                error += (i128::from(target) - reconstructed).pow(2);
            }
        }
        eprintln!(
            "prefix frontier budget={budget} bits={} indices={:?} signal={signal} distortion={error} snr={}",
            range.tell(),
            candidate.previous_index,
            10.0 * (signal as f64 / error as f64).log10()
        );
    }
}

// Fixed winning gains, three pressures, four spectral models: no production
// selection changes and no additional gain grid. LTP is diagnostic only.
fn model_diagnostics(previous: &SilkEncoder, selected: &SilkEncoder, input: &[f32]) {
    for channel in 0..2 {
        let samples: Vec<i32> = input
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| {
                let left = (f64::from(frame[0]) * 32768.0).round() as i32;
                let right = (f64::from(frame[1]) * 32768.0).round() as i32;
                if channel == 0 {
                    left.midpoint(right)
                } else {
                    (left - right) / 2
                }
            })
            .collect();
        let mut history: Vec<i32> = previous.history[channel]
            .iter()
            .map(|&x| {
                ((i64::from(x) * i64::from(previous.gain[channel])) >> 30).clamp(-32768, 32767)
                    as i32
            })
            .collect();
        history.extend_from_slice(&samples);
        for (name, analysis, bitrate) in [
            (
                "production",
                history.as_slice(),
                previous.config.bitrate / 2,
            ),
            ("raw_history", history.as_slice(), 48_000),
            ("raw_current", samples.as_slice(), 48_000),
            (
                "sparse_current",
                samples.as_slice(),
                previous.config.bitrate / 2,
            ),
        ] {
            let model = analyze_rate(analysis, Bandwidth::Wideband, bitrate).unwrap();
            let pitch =
                prediction::analyze(&samples, &previous.pcm_history[channel], &model.lpc, 16000);
            let energy = samples.iter().map(|&x| i128::from(x).pow(2)).sum::<i128>();
            eprintln!(
                "model channel={channel} name={name} signal={energy} prediction_error={} lag={:?}",
                prediction_error(&samples, &model.lpc),
                pitch.as_ref().map(|p| p.lag)
            );
            for pressure in [0, 4, 8] {
                for voiced in [false, true] {
                    if voiced && pitch.is_none() {
                        continue;
                    }
                    diagnostic_model_cost(
                        previous,
                        selected,
                        &samples,
                        channel,
                        &model,
                        pitch.as_ref().filter(|_| voiced),
                        pressure,
                    );
                }
            }
        }
    }
}

fn diagnostic_model_cost(
    previous: &SilkEncoder,
    selected: &SilkEncoder,
    samples: &[i32],
    channel: usize,
    model: &Model,
    pitch: Option<&Pitch>,
    pressure: u32,
) {
    let (pulses, _, pcm) = quantize(QuantizerInput {
        input: samples,
        lpc: &model.lpc,
        history: &previous.history[channel],
        old_gain: previous.gain[channel],
        gain: selected.gain[channel],
        pitch,
        previous_pcm: &previous.pcm_history[channel],
        rate: 16000,
        bitrate: previous.config.bitrate / 2,
        rate_pressure: pressure,
        optimize_pulse_cost: true,
        unshaped: false,
    });
    let distortion = samples
        .iter()
        .zip(&pcm)
        .map(|(&x, &y)| (i128::from(x) - i128::from(y)).pow(2))
        .sum::<i128>();
    let mut range = RangeEncoder::new(1024).unwrap();
    encode_parameters(
        &mut range,
        FrameParameters {
            model,
            gain: selected.previous_index[channel],
            previous_gain: previous.previous_index[channel],
            long: samples.len() == 320,
            bandwidth: Bandwidth::Wideband,
            pitch,
            rate: 16000,
        },
    )
    .unwrap();
    let parameter_bits = range.tell();
    encode_pulses(&mut range, &pulses, pitch.is_some()).unwrap();
    eprintln!(
        "model RD channel={channel} stage1={} voiced={} pressure={pressure} index={} parameter_bits={parameter_bits} total_bits={} distortion={distortion}",
        model.stage1,
        pitch.is_some(),
        selected.previous_index[channel],
        range.tell()
    );
}
