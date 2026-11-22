//! Every-callback allocation and persistent synthesis differential.
use super::*;
use crate::encoder::hybrid::{NativePrefixScore, NativeScoreCache};

struct Differential {
    short: NativePrefixScore,
    long: NativePrefixScore,
    calls: usize,
}

impl PrefixCandidateScorer for Differential {
    fn score(&mut self, config: EncoderConfig, bytes: &[u8]) -> Result<i128> {
        let mut padded = bytes.to_vec();
        padded.resize(65536, 0);
        let mut a = self.short.incoming.clone();
        let mut b = self.long.incoming.clone();
        assert_eq!(
            self.short.decode(&mut a, config, bytes)?,
            self.long.decode(&mut b, config, &padded)?
        );
        // A second frame observes synthesis, LPC/LTP and resampler state after
        // the candidate, not just its current output or range tell.
        assert_eq!(
            self.short.decode(&mut a, config, bytes)?,
            self.long.decode(&mut b, config, &padded)?
        );
        let score = self.short.score(config, bytes)?;
        assert_eq!(score, self.long.score(config, &padded)?);
        self.calls += 1;
        Ok(score)
    }
}

#[test]
fn replay_every_callback_and_winner_exact() {
    for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
        for disabled in [false, true] {
            let count = super::super::channel_count(channels);
            let config = EncoderConfig {
                sample_rate: SampleRate::Hz16000,
                channels,
                frame_size: 160,
                bitrate: 24000 * count as u32,
            };
            let input: Vec<_> = (0..160)
                .flat_map(|i| {
                    let x = (std::f32::consts::TAU * i as f32 * 143.0 / 16000.0).sin() * 0.1;
                    [x, -0.7 * x].into_iter().take(count)
                })
                .collect();
            let make = || NativePrefixScore {
                incoming: crate::Decoder::new(SampleRate::Hz48000, channels).unwrap(),
                target: vec![0; 480 * count],
                best: None,
                cache: {
                    let mut cache = NativeScoreCache::default();
                    cache.disabled = disabled;
                    cache
                },
            };
            let mut score = Differential {
                short: make(),
                long: make(),
                calls: 0,
            };
            let mut a = SilkEncoder::new(config).unwrap();
            let mut b = a.clone();
            a.quantizer_cache_disabled = disabled;
            b.quantizer_cache_disabled = disabled;
            let mut range_a = RangeEncoder::new(512).unwrap();
            let mut range_b = range_a.clone();
            replay::DIFFERENTIAL.set(true);
            replay::CHECKED.set(0);
            a.encode_prefix_scored(&input, &mut range_a, &mut score)
                .unwrap();
            replay::DIFFERENTIAL.set(false);
            assert!(replay::CHECKED.get() >= score.calls);
            replay::LONG.set(true);
            let mut long = make();
            b.encode_prefix_scored(&input, &mut range_b, &mut long)
                .unwrap();
            replay::LONG.set(false);
            assert_eq!(range_a.tell(), range_b.tell());
            assert_eq!(range_a.finish().unwrap(), range_b.finish().unwrap());
            assert_eq!(a.history, b.history);
            assert_eq!(a.gain, b.gain);
            assert_eq!(a.pcm_history, b.pcm_history);
            assert_eq!(a.previous_index, b.previous_index);
            let short = score.short.best.unwrap();
            let reference = long.best.unwrap();
            assert_eq!(short.0, reference.0);
            assert_eq!(short.1, reference.1[..short.1.len()]);
            assert!(reference.1[short.1.len()..].iter().all(|&x| x == 0));
            eprintln!(
                "REPLAY_DIFFERENTIAL channels={count} cache_disabled={disabled} callbacks={}",
                score.calls
            );
        }
    }
}
