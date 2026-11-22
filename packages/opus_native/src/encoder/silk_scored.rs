//! Bounded cold-mono replay equivalence and scorer transaction tests.
use super::*;

fn decode_replay(config: EncoderConfig, payload: &[u8], hybrid: bool) -> Vec<i16> {
    let mut range = crate::range::RangeDecoder::new(payload).unwrap();
    if hybrid {
        // Hybrid's leading CELT silence flag precedes the SILK header.
        assert!(!range.ec_dec_bit_logp(15).unwrap());
    }
    let mut decoder = SilkDecoder::new(
        config.sample_rate,
        config.channels,
        (config.frame_size / 16) as u8,
    )
    .unwrap();
    decoder.begin_packet();
    decoder
        .decode_header_bits(&mut range, config.channels == crate::Channels::Stereo)
        .unwrap();
    let mut pcm = vec![0; config.frame_size * super::super::channel_count(config.channels)];
    assert_eq!(
        decoder
            .decode_silk_frame(&mut range, true, &mut pcm)
            .unwrap(),
        config.frame_size
    );
    pcm
}

fn resample(pcm: &[i16], channels: usize) -> Vec<i16> {
    let planes: Vec<_> = (0..channels)
        .map(|channel| {
            let mut resampler = crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap();
            resampler
                .process(
                    &pcm.iter()
                        .skip(channel)
                        .step_by(channels)
                        .copied()
                        .collect::<Vec<_>>(),
                )
                .unwrap()
        })
        .collect();
    (0..planes[0].len())
        .flat_map(|i| planes.iter().map(move |plane| plane[i]))
        .collect()
}

struct ColdScore {
    calls: usize,
    best: Option<(i128, Vec<i16>, Vec<i16>)>,
}

impl PrefixCandidateScorer for ColdScore {
    fn score(&mut self, config: EncoderConfig, replay: &[u8]) -> Result<i128> {
        let pcm = decode_replay(config, replay, false);
        let channels = super::super::channel_count(config.channels);
        let native = resample(&pcm, channels);
        // Quiet cold proof: use a fixed native-rate zero target, no fitted
        // alignment or gain and no live synthesis state mutation.
        let error = native.iter().map(|&x| i128::from(x).pow(2)).sum();
        self.calls += 1;
        if self.best.as_ref().is_none_or(|best| error < best.0) {
            self.best = Some((error, pcm, native));
        }
        Ok(error)
    }
}

#[test]
fn scored_cold_mono_replay_matches_shared_prefix() {
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Mono,
        frame_size: 160,
        bitrate: 16000,
    };
    let input: Vec<_> = (0..160)
        .map(|i| if i % 17 < 8 { 0.0001 } else { -0.0001 })
        .collect();
    let mut encoder = SilkEncoder::new(config).unwrap();
    let mut range = RangeEncoder::new(256).unwrap();
    range.ec_enc_bit_logp(false, 15).unwrap();
    let initial = range.tell();
    let mut scorer = ColdScore {
        calls: 0,
        best: None,
    };
    encoder
        .encode_prefix_scored(&input, &mut range, &mut scorer)
        .unwrap();
    assert!(range.tell() - initial <= 160);
    assert!(scorer.calls > 1);
    // Append suffix entropy to prove replay is independent of the shared
    // stream's termination; the prefix decoder does not consume that suffix.
    range.ec_enc_uint(7, 19).unwrap();
    let payload = range.finish().unwrap();
    let pcm = decode_replay(config, &payload, true);
    let (_, expected, expected_native) = scorer.best.unwrap();
    assert_eq!(pcm, expected);
    let mut resampler = crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap();
    assert_eq!(resampler.process(&pcm).unwrap(), expected_native);
    eprintln!(
        "SILK_SCORED_REPLAY calls={} exact_internal=160 exact_native=480",
        scorer.calls
    );
}

#[test]
fn scored_error_is_atomic() {
    struct Reject;
    impl PrefixCandidateScorer for Reject {
        fn score(&mut self, _: EncoderConfig, _: &[u8]) -> Result<i128> {
            Err(Error::Unsupported("scorer sentinel".into()))
        }
    }
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Mono,
        frame_size: 160,
        bitrate: 16000,
    };
    let mut encoder = SilkEncoder::new(config).unwrap();
    let original = encoder.clone();
    let mut range = RangeEncoder::new(256).unwrap();
    let before = range.clone().finish().unwrap();
    let result = encoder.encode_prefix_scored(&[0.0; 160], &mut range, &mut Reject);
    assert!(matches!(result, Err(Error::Unsupported(message)) if message == "scorer sentinel"));
    assert_eq!(range.finish().unwrap(), before);
    assert_eq!(encoder.history, original.history);
    assert_eq!(encoder.gain, original.gain);
    assert_eq!(encoder.previous_index, original.previous_index);
    assert_eq!(encoder.pcm_history, original.pcm_history);
}

#[test]
fn scored_cold_stereo_replay_matches_shared_prefix() {
    for frame_size in [160, 320] {
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size,
            bitrate: 24000,
        };
        let input: Vec<_> = (0..frame_size)
            .flat_map(|i| {
                let x = if i % 17 < 8 { 0.0001 } else { -0.0001 };
                [x, -x * 0.7]
            })
            .collect();
        let mut encoder = SilkEncoder::new(config).unwrap();
        let mut range = RangeEncoder::new(256).unwrap();
        range.ec_enc_bit_logp(false, 15).unwrap();
        let initial = range.tell();
        let mut baseline = encoder.clone();
        let mut baseline_range = range.clone();
        baseline
            .encode_prefix_ranked(&input, &mut baseline_range)
            .unwrap();
        let baseline_payload = baseline_range.finish().unwrap();
        let baseline_pcm = decode_replay(config, &baseline_payload, true);
        let baseline_error: i128 = resample(&baseline_pcm, 2)
            .iter()
            .map(|&x| i128::from(x).pow(2))
            .sum();
        let mut scorer = ColdScore {
            calls: 0,
            best: None,
        };
        encoder
            .encode_prefix_scored(&input, &mut range, &mut scorer)
            .unwrap();
        assert!(range.tell() - initial <= (u64::from(config.bitrate) * frame_size as u64 / 16000));
        range.ec_enc_uint(7, 19).unwrap();
        let pcm = decode_replay(config, &range.finish().unwrap(), true);
        let (error, expected, expected_native) = scorer.best.unwrap();
        assert_eq!(pcm, expected);
        assert_eq!(resample(&pcm, 2), expected_native);
        assert!(
            error <= baseline_error,
            "native baseline must never regress"
        );
        eprintln!(
            "SILK_STEREO_SCORED frame_size={frame_size} calls={} baseline={baseline_error} selected={error}",
            scorer.calls
        );
    }
}

#[test]
fn scored_stereo_error_is_atomic() {
    struct Reject;
    impl PrefixCandidateScorer for Reject {
        fn score(&mut self, _: EncoderConfig, _: &[u8]) -> Result<i128> {
            Err(Error::Unsupported("stereo scorer sentinel".into()))
        }
    }
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Stereo,
        frame_size: 160,
        bitrate: 24000,
    };
    let mut encoder = SilkEncoder::new(config).unwrap();
    let original = encoder.clone();
    let mut range = RangeEncoder::new(256).unwrap();
    let before = range.clone().finish().unwrap();
    let result = encoder.encode_prefix_scored(&[0.0; 320], &mut range, &mut Reject);
    assert!(
        matches!(result, Err(Error::Unsupported(message)) if message == "stereo scorer sentinel")
    );
    assert_eq!(range.finish().unwrap(), before);
    assert_eq!(encoder.history, original.history);
    assert_eq!(encoder.gain, original.gain);
    assert_eq!(encoder.previous_index, original.previous_index);
    assert_eq!(encoder.pcm_history, original.pcm_history);
}

#[test]
#[cfg(feature = "hybrid")]
fn native_replay_cache_is_exact() {
    use super::super::hybrid::{NativePrefixScore, NativeScoreCache};
    for (channels, bitrate, scale) in [
        (crate::Channels::Mono, 24000, 0.01_f32),
        (crate::Channels::Mono, 48000, 0.01),
        (crate::Channels::Stereo, 24000, 0.2),
    ] {
        let count = super::super::channel_count(channels);
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels,
            frame_size: 160,
            bitrate,
        };
        let input: Vec<_> = (0..160)
            .flat_map(|i| {
                let x = (std::f32::consts::TAU * 143.0 * i as f32 / 16000.0).sin() * scale;
                [x, x * -0.7].into_iter().take(count)
            })
            .collect();
        let target: Vec<_> = input
            .chunks_exact(count)
            .flat_map(|frame| {
                (0..3).flat_map(move |_| frame.iter().map(|&x| (x * 32768.0).round() as i16))
            })
            .collect();
        let make = |disabled| NativePrefixScore {
            incoming: crate::Decoder::new(SampleRate::Hz48000, channels).unwrap(),
            target: target.clone(),
            best: None,
            cache: {
                let mut cache = NativeScoreCache::default();
                cache.disabled = disabled;
                cache
            },
        };
        let mut cached = make(false);
        let mut original = make(true);
        let mut a = SilkEncoder::new(config).unwrap();
        let mut b = a.clone();
        let mut range_a = RangeEncoder::new(512).unwrap();
        let mut range_b = range_a.clone();
        let started = std::time::Instant::now();
        a.encode_prefix_scored(&input, &mut range_a, &mut cached)
            .unwrap();
        let cached_us = started.elapsed().as_micros();
        let started = std::time::Instant::now();
        REFERENCE_PAIRS.set(true);
        b.encode_prefix_scored(&input, &mut range_b, &mut original)
            .unwrap();
        REFERENCE_PAIRS.set(false);
        let original_us = started.elapsed().as_micros();
        assert_eq!(range_a.finish().unwrap(), range_b.finish().unwrap());
        assert_eq!(a.history, b.history);
        assert_eq!(a.gain, b.gain);
        assert_eq!(a.previous_index, b.previous_index);
        assert_eq!(a.pcm_history, b.pcm_history);
        let best = cached.best.clone().unwrap();
        let reference = original.best.as_ref().unwrap();
        assert_eq!(best.0, reference.0);
        assert_eq!(best.1, reference.1);
        assert_eq!(
            cached
                .decode(&mut cached.incoming.clone(), config, &best.1)
                .unwrap(),
            original
                .decode(&mut original.incoming.clone(), config, &reference.1)
                .unwrap()
        );
        assert_eq!(cached.cache.calls, original.cache.calls);
        let (unique, bytes) = cached.cache.sizes();
        eprintln!(
            "NATIVE_CACHE channels={count} bitrate={bitrate} calls={} unique={unique} hits={} key_bytes={bytes} original_us={original_us} cached_us={cached_us}",
            cached.cache.calls, cached.cache.hits
        );
        // A hit still executes strict winner bookkeeping, even if a caller
        // clears diagnostic best state without changing target/incoming state.
        cached.best = None;
        assert_eq!(cached.score(config, &best.1).unwrap(), best.0);
        assert_eq!(cached.best.as_ref().unwrap().1, best.1);
        // Errors are never cached.
        let before = cached.cache.sizes();
        assert!(cached.score(config, &[]).is_err());
        assert!(cached.score(config, &[]).is_err());
        assert_eq!(cached.cache.sizes(), before);
    }
}
