//! SILK encoder with integer LPC/pitch analysis and noise-shaped excitation.
#[path = "silk_prediction.rs"]
mod prediction;
use super::{EncoderConfig, entropy};
use crate::range::RangeEncoder;
use crate::silk::{self, SilkDecoder};
use crate::{Bandwidth, Error, Result, SampleRate};
use prediction::Pitch;
#[cfg(test)]
#[path = "silk_cold.rs"]
mod cold;
#[cfg(test)]
#[path = "silk_completion.rs"]
mod completion;
#[cfg(test)]
#[path = "silk_converted.rs"]
mod converted;
#[cfg(test)]
#[path = "silk_public.rs"]
mod public;
#[cfg(test)]
#[path = "silk_qualification.rs"]
mod qualification;

#[derive(Clone)]
pub(super) struct SilkEncoder {
    config: EncoderConfig,
    history: [[i32; 16]; 2],
    gain: [i32; 2],
    previous_index: [i32; 2],
    pcm_history: [Vec<i16>; 2],
    #[cfg(test)]
    transmitted_voiced: usize,
}

type QuantizedFrame = (Vec<i16>, [i32; 16], Vec<i16>);

struct PacketCandidate {
    distortion: i128,
    payload: Vec<u8>,
    prefix: Option<RangeEncoder>,
    indices: Vec<i32>,
    gains: Vec<i32>,
    quantized: Vec<QuantizedFrame>,
}

struct Model {
    stage1: u8,
    residual: Vec<i8>,
    lpc: Vec<i16>,
}

impl SilkEncoder {
    pub(super) fn new(config: EncoderConfig) -> Result<Self> {
        let rate = sample_rate(config.sample_rate)?;
        if ![rate / 100, rate / 50, rate / 25, rate * 3 / 50].contains(&config.frame_size)
            || !(8_000..=128_000).contains(&config.bitrate)
        {
            return Err(Error::Unsupported(
                "SILK encoding requires 8/12/16 kHz, 10/20/40/60 ms and 8..=128 kbit/s".into(),
            ));
        }
        Ok(Self {
            config,
            history: [[0; 16]; 2],
            gain: [65536; 2],
            previous_index: [10; 2],
            pcm_history: [Vec::new(), Vec::new()],
            #[cfg(test)]
            transmitted_voiced: 0,
        })
    }

    /// Reset predictive state at a decoder-synchronized SILK stream boundary.
    /// This is not a substitute for Opus transition/redundancy signaling.
    pub(super) fn reset(&mut self) {
        self.history = [[0; 16]; 2];
        self.gain = [65536; 2];
        self.previous_index = [10; 2];
        self.pcm_history = [Vec::new(), Vec::new()];
        #[cfg(test)]
        {
            self.transmitted_voiced = 0;
        }
    }

    /// Change the excitation budget without discarding LPC/pitch history.
    pub(super) fn set_bitrate(&mut self, bitrate: u32) -> Result<()> {
        if !(8_000..=128_000).contains(&bitrate) {
            return Err(Error::Unsupported(
                "SILK bitrate must be 8..=128 kbit/s".into(),
            ));
        }
        self.config.bitrate = bitrate;
        Ok(())
    }

    pub(super) fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        let rate = sample_rate(self.config.sample_rate)?;
        if self.config.frame_size <= rate / 50 {
            return self.encode_internal(pcm, None, false);
        }
        // Use ordinary Opus VBR framing of independently coded 20 ms SILK
        // frames. Predictor state persists across the constituent frames;
        // unlike SILK internal 40/60 ms framing this needs no conditional
        // gain/pitch syntax and is equally valid on the wire.
        let original = self.config;
        let count = original.frame_size / (rate / 50);
        let mut next = self.clone();
        next.config.frame_size = rate / 50;
        // With one-byte frame lengths the aggregate overhead versus N
        // standalone packets is one byte; with two-byte lengths it is N.
        // Reserve that exact worst-case overhead over the packet duration.
        let two_byte_lengths = original.bitrate as usize / 400 >= 252;
        let overhead = if two_byte_lengths { count } else { 1 };
        let reserve = (overhead * 8 * rate).div_ceil(original.frame_size) as u32;
        next.config.bitrate = original.bitrate.saturating_sub(reserve);
        let mut frames = Vec::new();
        for frame in
            pcm.chunks_exact(next.config.frame_size * super::channel_count(original.channels))
        {
            frames.push(next.encode_internal(frame, None, false)?);
        }
        if frames.len() != count {
            return Err(Error::Unsupported("invalid long SILK input length".into()));
        }
        let mut packet = vec![frames[0][0] | 3, 0x80 | count as u8];
        for frame in frames.iter().take(count - 1) {
            let length = frame.len() - 1;
            if length < 252 {
                packet.push(length as u8);
            } else {
                let first = 252 + (length & 3);
                packet.push(first as u8);
                packet.push(((length - first) / 4) as u8);
            }
        }
        for frame in &frames {
            packet.extend_from_slice(&frame[1..]);
        }
        if packet.len() * rate * 8 > original.bitrate as usize * original.frame_size {
            return Err(Error::Unsupported(
                "long SILK packet exceeds bitrate budget".into(),
            ));
        }
        next.config = original;
        *self = next;
        Ok(packet)
    }

    #[cfg(feature = "hybrid")]
    pub(super) fn encode_prefix(&mut self, pcm: &[f32], encoder: &mut RangeEncoder) -> Result<()> {
        if self.config.frame_size > sample_rate(self.config.sample_rate)? / 50 {
            return Err(Error::Unsupported(
                "shared SILK prefix requires 10/20 ms".into(),
            ));
        }
        self.encode_internal(pcm, Some(encoder), false).map(|_| ())
    }

    /// Rank shared-prefix candidates by reconstructed PCM distortion before
    /// atomically committing the chosen entropy and predictive state.
    /// The caller supplies the lowband budget through `set_bitrate` first.
    #[cfg(feature = "hybrid")]
    pub(super) fn encode_prefix_ranked(
        &mut self,
        pcm: &[f32],
        encoder: &mut RangeEncoder,
    ) -> Result<()> {
        if self.config.frame_size > sample_rate(self.config.sample_rate)? / 50 {
            return Err(Error::Unsupported(
                "shared SILK prefix requires 10/20 ms".into(),
            ));
        }
        self.encode_internal(pcm, Some(encoder), true).map(|_| ())
    }

    fn commit_quantized(
        &mut self,
        quantized: &[QuantizedFrame],
        gains: &[i32],
        indices: &[i32],
        rate: usize,
    ) {
        for (channel, (_, history, pcm)) in quantized.iter().enumerate() {
            self.history[channel] = *history;
            self.gain[channel] = gains[channel];
            self.previous_index[channel] = indices[channel];
            self.pcm_history[channel].extend_from_slice(pcm);
            let excess = self.pcm_history[channel].len().saturating_sub(rate / 50);
            self.pcm_history[channel].drain(..excess);
        }
    }

    // Zero input makes every open-loop LPC prediction error exactly zero.
    // Resolve that tie using the mandatory offset excitation and committed
    // synthesis history, not an arbitrary first spectral codebook entry.
    fn silence_model(
        &self,
        input: &[i32],
        channel: usize,
        bandwidth: Bandwidth,
        rate: usize,
    ) -> Result<Model> {
        let gain = SilkDecoder::dequantize_gain((self.previous_index[channel] - 16).max(0));
        let order = if bandwidth == Bandwidth::Wideband {
            16
        } else {
            10
        };
        let mut best = None;
        let mut best_error = i128::MAX;
        for stage1 in 0..32 {
            let residual = vec![0; order];
            let lsf = SilkDecoder::reconstruct_lsf(stage1, &residual, bandwidth)?;
            let lpc = SilkDecoder::limit_lpc_coefficients(&lsf, bandwidth)?;
            let (_, _, pcm) = quantize(QuantizerInput {
                input,
                lpc: &lpc,
                history: &self.history[channel],
                old_gain: self.gain[channel],
                gain,
                pitch: None,
                previous_pcm: &self.pcm_history[channel],
                rate,
                bitrate: self.config.bitrate / super::channel_count(self.config.channels) as u32,
                rate_pressure: 0,
                optimize_pulse_cost: false,
            });
            let error = pcm.iter().map(|&x| i128::from(x).pow(2)).sum();
            if error < best_error {
                best_error = error;
                best = Some(Model {
                    stage1,
                    residual,
                    lpc,
                });
            }
        }
        Ok(best.expect("SILK spectral codebook is nonempty"))
    }

    fn encode_internal(
        &mut self,
        pcm: &[f32],
        shared: Option<&mut RangeEncoder>,
        rank_prefix: bool,
    ) -> Result<Vec<u8>> {
        if rank_prefix
            && self.config.channels == crate::Channels::Stereo
            && self.config.bitrate <= 32_000
        {
            return self
                .encode_model_ranked(pcm, shared.expect("ranked prefix requires entropy state"));
        }
        self.encode_search(pcm, shared, rank_prefix, 0)
    }

    // Every spectral combination starts from the identical committed predictor
    // and real incoming range state. Fit and rank the transmitted reconstruction,
    // not open-loop LPC prediction error. The baseline model is always included.
    fn encode_model_ranked(&mut self, pcm: &[f32], shared: &mut RangeEncoder) -> Result<Vec<u8>> {
        let mut best: Option<(i128, Self, RangeEncoder)> = None;
        let mut failure = None;
        for mask in 0..4 {
            let mut candidate = self.clone();
            let mut range = shared.clone();
            if let Err(error) = candidate.encode_search(pcm, Some(&mut range), true, mask) {
                failure = Some(error);
                continue;
            }
            let mut distortion = 0_i128;
            for (i, frame) in pcm.as_chunks::<2>().0.iter().enumerate() {
                let left = (f64::from(frame[0]) * 32768.0).round() as i32;
                let right = (f64::from(frame[1]) * 32768.0).round() as i32;
                for (channel, target) in [left.midpoint(right), (left - right) / 2]
                    .into_iter()
                    .enumerate()
                {
                    let history = &candidate.pcm_history[channel];
                    let reconstructed = history[history.len() - self.config.frame_size + i];
                    distortion += (i128::from(target) - i128::from(reconstructed)).pow(2);
                }
            }
            #[cfg(test)]
            eprintln!(
                "prefix model mask={mask} bits={} distortion={distortion}",
                range.tell()
            );
            if best.as_ref().is_none_or(|previous| distortion < previous.0) {
                best = Some((distortion, candidate, range));
            }
        }
        if let Some((_, candidate, range)) = best {
            *self = candidate;
            *shared = range;
            Ok(Vec::new())
        } else {
            Err(failure.expect("all spectral candidates failed"))
        }
    }

    #[allow(clippy::too_many_lines)] // Transactional SILK entropy budget search.
    fn encode_search(
        &mut self,
        pcm: &[f32],
        mut shared: Option<&mut RangeEncoder>,
        rank_prefix: bool,
        model_mask: u8,
    ) -> Result<Vec<u8>> {
        let rate = sample_rate(self.config.sample_rate)?;
        let bandwidth = bandwidth(self.config.sample_rate);
        let channels = super::channel_count(self.config.channels);
        let mut inputs = vec![Vec::with_capacity(self.config.frame_size); channels];
        for frame in pcm.chunks_exact(channels) {
            let left = (f64::from(frame[0]) * 32768.0).round() as i32;
            if channels == 1 {
                inputs[0].push(left);
            } else {
                let right = (f64::from(frame[1]) * 32768.0).round() as i32;
                inputs[0].push(left.midpoint(right));
                inputs[1].push((left - right) / 2);
            }
        }
        let models: Vec<_> = inputs
            .iter()
            .enumerate()
            .map(|(channel, input)| {
                if input.iter().all(|&sample| sample == 0) {
                    return self.silence_model(input, channel, bandwidth, rate);
                }
                let mut analysis: Vec<i32> = self.history[channel]
                    .iter()
                    .map(|&x| {
                        ((i64::from(x) * i64::from(self.gain[channel])) >> 30).clamp(-32768, 32767)
                            as i32
                    })
                    .collect();
                analysis.extend_from_slice(input);
                let bitrate = self.config.bitrate / channels as u32;
                analyze_rate(
                    &analysis,
                    bandwidth,
                    if model_mask & (1 << channel) != 0 {
                        48_000
                    } else {
                        bitrate
                    },
                )
            })
            .collect::<Result<_>>()?;
        let pitches: Vec<_> = inputs
            .iter()
            .enumerate()
            .map(|(c, input)| {
                // Keep hybrid and stereo on the established unvoiced path until
                // their predictor/delay adaptation is qualified independently.
                if channels == 1 && shared.is_none() {
                    prediction::analyze(input, &self.pcm_history[c], &models[c].lpc, rate)
                } else {
                    None
                }
            })
            .collect();
        let bytes =
            (self.config.bitrate as usize * self.config.frame_size / (rate * 8)).saturating_sub(1);
        // Actual entropy budgeting with transactional reconstruction. Raising
        // the common gain reduces excitation density without packet truncation.
        let mut last_failure = String::new();
        // Packet fit alone is insufficient at high rates: a very fine gain
        // can fit saturated pulses while failing to represent the input.
        // Rank standalone candidates by actual reconstructed PCM distortion
        // at every rate, not only in the low-rate policy.
        let optimize_distortion = shared.is_none() || rank_prefix;
        let refine_low_rate =
            optimize_distortion && self.config.bitrate / channels as u32 <= 24_000;
        let mut best: Option<PacketCandidate> = None;
        let mut first_fit = None;
        // If the maximum legal gain is not enough, increase the RD pulse
        // penalty independently. Every candidate is synthesized from the
        // same committed state; no entropy bytes or pulses are truncated.
        // Short low-rate prefixes need finer gain spacing as well as pulse
        // sparsity: the coarse four-index steps can skip a better-fitting
        // reconstruction gain. Cover the rate window, rather than one exact
        // budget, while keeping standalone search behavior unchanged.
        let per_channel_bitrate = self.config.bitrate / channels as u32;
        let refine_short_frame = optimize_distortion
            && (per_channel_bitrate == 12_000 || (rank_prefix && per_channel_bitrate < 24_000))
            && self.config.frame_size <= rate / 50
            && rate > 8000;
        let trials = if refine_short_frame {
            124
        } else if refine_low_rate {
            108
        } else {
            76
        };
        // A common mid/side gain forces the quieter component to use the
        // louder component's quantization step. Add RMS-scaled gain lattices
        // without removing the equal-gain baseline. Derive the offset using
        // the actual transmitted gain table (no floating-point logarithms).
        let mut gain_offsets = vec![[0_i32; 2]];
        if channels == 2 && (rank_prefix || (shared.is_none() && per_channel_bitrate <= 24_000)) {
            let energy: Vec<i128> = inputs
                .iter()
                .map(|input| {
                    input
                        .iter()
                        .map(|&x| i128::from(x) * i128::from(x))
                        .sum::<i128>()
                        .max(1)
                })
                .collect();
            let quiet = usize::from(energy[1] < energy[0]);
            let loud = 1 - quiet;
            let target = (energy[quiet] << 24) / energy[loud];
            let reference = i128::from(SilkDecoder::dequantize_gain(32));
            let offset = (-16..=0)
                .min_by_key(|&offset| {
                    let gain = i128::from(SilkDecoder::dequantize_gain(32 + offset));
                    (((gain * gain) << 24) / (reference * reference) - target).abs()
                })
                .unwrap_or(0);
            for offset in [offset / 2, offset] {
                let mut pair = [0; 2];
                pair[quiet] = offset;
                if !gain_offsets.contains(&pair) {
                    gain_offsets.push(pair);
                }
            }
        }
        let baseline_trials = trials;
        // Sparse high-pressure fits must also be searched at useful gains.
        // Otherwise a sharp LPC model may fit only at gain 63, where even
        // mandatory offset excitation can ring and clip throughout a frame.
        // Retain all baseline candidates and append eight finer gains at
        // twelve independent pulse penalties, plus the one/two-index
        // neighborhood at pressures zero through twelve. This applies to
        // ranked stereo at all budgets and standalone low-rate coding.
        let independent_pressure =
            (rank_prefix && channels == 2) || (shared.is_none() && per_channel_bitrate <= 24_000);
        let baseline_search_trials = baseline_trials + if independent_pressure { 122 } else { 0 };
        // The sparse search is anchored to the first fit at gain 63. On a
        // persistent predictor, useful fits can lie below its eight coarse
        // refinement gains, or between them at a different pulse pressure.
        // Cover every legal gain at each existing pressure for low-rate
        // shared prefixes, retaining every historical candidate. Do not
        // change standalone or accepted high-rate prefix search policy.
        let complete_prefix_search = rank_prefix && per_channel_bitrate <= 24_000;
        let trials = baseline_search_trials + if complete_prefix_search { 64 * 13 } else { 0 };
        let common_trials = trials * gain_offsets.len() as i32;
        // Refine one component while retaining the other's exact winning
        // pulses. A shared pulse pressure cannot allocate bits independently
        // even when both RMS gain offsets are nearly zero.
        let coordinate_trials = if rank_prefix && channels == 2 && per_channel_bitrate <= 16_000 {
            2 * 2 * 17 * 13
        } else {
            0
        };
        let mut coordinate_anchor = [0_i32; 2];
        for search_trial in 0..common_trials + coordinate_trials {
            let coordinate = search_trial >= common_trials;
            if coordinate && best.is_none() {
                break;
            }
            let trial = if coordinate {
                search_trial - common_trials
            } else {
                search_trial % trials
            };
            let offsets = if coordinate {
                [0; 2]
            } else {
                gain_offsets[(search_trial / trials) as usize]
            };
            let coordinate_channel = (trial / (17 * 13)) as usize % 2;
            if trial == 0 {
                first_fit = None;
            }
            if coordinate && trial % (17 * 13) == 0 {
                coordinate_anchor.copy_from_slice(
                    &best
                        .as_ref()
                        .expect("coordinate search requires a fit")
                        .indices,
                );
            }
            let (gain_index, rate_pressure) = if coordinate {
                (
                    coordinate_anchor[coordinate_channel] + (trial / 13) % 17 - 8,
                    (trial % 13) as u32,
                )
            } else if trial >= baseline_search_trials {
                let refinement = trial - baseline_search_trials;
                (refinement / 13, (refinement % 13) as u32)
            } else if trial >= baseline_trials + 96 {
                let refinement = trial - baseline_trials - 96;
                (
                    first_fit.unwrap_or(63) - 1 - refinement / 13,
                    (refinement % 13) as u32,
                )
            } else if trial >= baseline_trials {
                let refinement = trial - baseline_trials;
                (
                    first_fit.unwrap_or(63) - 4 * (1 + refinement / 12),
                    1 + (refinement % 12) as u32,
                )
            } else if trial < 76 {
                (trial.min(63), (trial - 63).max(0) as u32)
            } else if trial < 108 {
                // Explore independent pulse sparsity at finer reconstruction
                // gains. Large gains amplify even the mandatory zero-pulse
                // offset, which a gain-only controller cannot remove.
                let refinement = trial - 76;
                (
                    first_fit.unwrap_or(63) - 4 * (1 + refinement / 8),
                    1 + (refinement % 8) as u32,
                )
            } else {
                // Fill the one/two-index gap near the first fit without an
                // unbounded search over every gain/penalty combination.
                let refinement = trial - 108;
                (
                    first_fit.unwrap_or(63) - 1 - refinement / 8,
                    1 + (refinement % 8) as u32,
                )
            };
            let indices: Vec<_> = (0..channels)
                .map(|c| {
                    let index = if coordinate && c != coordinate_channel {
                        best.as_ref()
                            .expect("coordinate search requires a fit")
                            .indices[c]
                    } else {
                        gain_index + offsets[c]
                    };
                    index.max(self.previous_index[c] - 16).clamp(0, 63)
                })
                .collect();
            let gains: Vec<_> = indices
                .iter()
                .map(|&index| SilkDecoder::dequantize_gain(index))
                .collect();
            let quantized: Vec<_> = (0..channels)
                .map(|c| {
                    if coordinate && c != coordinate_channel {
                        return best
                            .as_ref()
                            .expect("coordinate search requires a fit")
                            .quantized[c]
                            .clone();
                    }
                    quantize(QuantizerInput {
                        input: &inputs[c],
                        lpc: &models[c].lpc,
                        history: &self.history[c],
                        old_gain: self.gain[c],
                        gain: gains[c],
                        pitch: pitches[c].as_ref(),
                        previous_pcm: &self.pcm_history[c],
                        rate,
                        bitrate: (self.config.bitrate / channels as u32).min(64000),
                        rate_pressure,
                        optimize_pulse_cost: rank_prefix
                            || (shared.is_none() && per_channel_bitrate <= 24_000),
                    })
                })
                .collect();
            let encode = |encoder: &mut RangeEncoder| -> Result<()> {
                for _ in 0..channels {
                    encoder.ec_enc_bit_logp(true, 1).map_err(entropy)?;
                    encoder.ec_enc_bit_logp(false, 1).map_err(entropy)?;
                }
                if channels == 2 {
                    use silk::constants::{
                        STEREO_WEIGHT_PDF_STAGE1, STEREO_WEIGHT_PDF_STAGE2,
                        STEREO_WEIGHT_PDF_STAGE3,
                    };
                    // Exact zero stereo predictors: both coarse indices select
                    // the -820..820 interval and its central interpolation.
                    encoder
                        .ec_enc_icdf(12, STEREO_WEIGHT_PDF_STAGE1, 8)
                        .map_err(entropy)?;
                    for _ in 0..2 {
                        encoder
                            .ec_enc_icdf(1, STEREO_WEIGHT_PDF_STAGE2, 8)
                            .map_err(entropy)?;
                        encoder
                            .ec_enc_icdf(2, STEREO_WEIGHT_PDF_STAGE3, 8)
                            .map_err(entropy)?;
                    }
                }
                for c in 0..channels {
                    encode_parameters(
                        encoder,
                        FrameParameters {
                            model: &models[c],
                            gain: indices[c],
                            previous_gain: self.previous_index[c],
                            long: self.config.frame_size == rate / 50,
                            bandwidth,
                            pitch: pitches[c].as_ref(),
                            rate,
                        },
                    )?;
                    encode_pulses(encoder, &quantized[c].0, pitches[c].is_some())?;
                }
                Ok(())
            };
            if rank_prefix {
                // A prefix is not a standalone packet: termination at its
                // measured length can fail even when the shared packet has
                // ample suffix capacity. Test the actual incoming range state
                // and charge only this prefix's incremental entropy usage.
                let incoming = shared.as_deref().expect("ranked prefix has entropy state");
                let initial_bits = incoming.tell();
                let mut candidate = incoming.clone();
                if let Err(error) = encode(&mut candidate) {
                    last_failure = format!("trial {trial}: shared-prefix encoding: {error}");
                    continue;
                }
                let prefix_bits = candidate.tell().saturating_sub(initial_bits);
                if prefix_bits as usize > bytes * 8 {
                    last_failure =
                        format!("trial {trial}: prefix {prefix_bits} bits > {}", bytes * 8);
                    continue;
                }
                first_fit.get_or_insert(gain_index);
                let distortion = inputs
                    .iter()
                    .zip(&quantized)
                    .map(|(input, (_, _, pcm))| {
                        input
                            .iter()
                            .zip(pcm)
                            .map(|(&original, &reconstructed)| {
                                let difference = i128::from(original) - i128::from(reconstructed);
                                difference * difference
                            })
                            .sum::<i128>()
                    })
                    .sum();
                if best
                    .as_ref()
                    .is_none_or(|previous| distortion < previous.distortion)
                {
                    #[cfg(test)]
                    if channels == 2 && self.config.bitrate <= 48_000 {
                        let errors: Vec<i128> = inputs
                            .iter()
                            .zip(&quantized)
                            .map(|(input, (_, _, pcm))| {
                                input
                                    .iter()
                                    .zip(pcm)
                                    .map(|(&x, &y)| (i128::from(x) - i128::from(y)).pow(2))
                                    .sum()
                            })
                            .collect();
                        let clipping: Vec<usize> = quantized
                            .iter()
                            .map(|(_, _, pcm)| {
                                pcm.iter()
                                    .filter(|&&x| x == i16::MIN || x == i16::MAX)
                                    .count()
                            })
                            .collect();
                        eprintln!(
                            "SILK_PREFIX_IMPROVEMENT trial={trial} coordinate={coordinate} channel={coordinate_channel} pressure={rate_pressure} offsets={offsets:?} indices={indices:?} bits={prefix_bits} distortion={distortion} errors={errors:?} clipped={clipping:?}"
                        );
                    }
                    best = Some(PacketCandidate {
                        distortion,
                        payload: Vec::new(),
                        prefix: Some(candidate),
                        indices,
                        gains,
                        quantized,
                    });
                }
                continue;
            }
            let mut encoder = RangeEncoder::new(bytes).map_err(entropy)?;
            if let Err(error) = encode(&mut encoder) {
                last_failure = format!(
                    "trial {trial}: {error}; pitch_lags={:?} nlsf_stage1={:?}; pulses max={:?} sum={:?} gains={gains:?}",
                    pitches
                        .iter()
                        .map(|pitch| pitch.as_ref().map(|pitch| pitch.lag))
                        .collect::<Vec<_>>(),
                    models.iter().map(|model| model.stage1).collect::<Vec<_>>(),
                    quantized
                        .iter()
                        .map(|(q, _, _)| q.iter().map(|q| q.unsigned_abs()).max())
                        .collect::<Vec<_>>(),
                    quantized
                        .iter()
                        .map(|(q, _, _)| q.iter().map(|q| u64::from(q.unsigned_abs())).sum::<u64>())
                        .collect::<Vec<_>>()
                );
                continue;
            }
            if encoder.tell() as usize > bytes * 8 {
                last_failure = format!("trial {trial}: {} bits > {}", encoder.tell(), bytes * 8);
                continue;
            }
            if !rank_prefix && let Some(shared) = shared.as_deref_mut() {
                encode(shared)?;
                #[cfg(test)]
                {
                    self.transmitted_voiced +=
                        pitches.iter().filter(|pitch| pitch.is_some()).count();
                }
                for c in 0..channels {
                    self.history[c] = quantized[c].1;
                    self.gain[c] = gains[c];
                    self.previous_index[c] = indices[c];
                    self.pcm_history[c].extend_from_slice(&quantized[c].2);
                    let excess = self.pcm_history[c].len().saturating_sub(rate / 50);
                    self.pcm_history[c].drain(..excess);
                }
                return Ok(Vec::new());
            }
            // SILK-only Opus implicitly interprets extra entropy bytes as CELT
            // redundancy. Re-encode at measured size instead of truncating.
            let exact_bytes = (encoder.tell() as usize).div_ceil(8);
            let mut encoder = RangeEncoder::new(exact_bytes).map_err(entropy)?;
            if let Err(error) = encode(&mut encoder) {
                // A measured-size candidate can still exhaust its byte
                // storage during range termination/carry handling. Reject
                // only this candidate; retain any previously fitted packet.
                last_failure = format!("trial {trial}: exact-size encoding: {error}");
                continue;
            }
            if let Ok(payload) = encoder.finish() {
                first_fit.get_or_insert(gain_index);
                let distortion = inputs
                    .iter()
                    .zip(&quantized)
                    .map(|(input, (_, _, pcm))| {
                        input
                            .iter()
                            .zip(pcm)
                            .map(|(&original, &reconstructed)| {
                                let difference = i128::from(original) - i128::from(reconstructed);
                                difference * difference
                            })
                            .sum::<i128>()
                    })
                    .sum();
                if best
                    .as_ref()
                    .is_none_or(|candidate| distortion < candidate.distortion)
                {
                    best = Some(PacketCandidate {
                        distortion,
                        payload,
                        prefix: None,
                        indices,
                        gains,
                        quantized,
                    });
                }
                if !optimize_distortion {
                    break;
                }
            }
        }
        if let Some(candidate) = best {
            if let Some(prefix) = candidate.prefix {
                *shared.expect("ranked prefix has entropy state") = prefix;
                self.commit_quantized(
                    &candidate.quantized,
                    &candidate.gains,
                    &candidate.indices,
                    rate,
                );
                return Ok(Vec::new());
            }
            #[cfg(test)]
            {
                self.transmitted_voiced += pitches.iter().filter(|pitch| pitch.is_some()).count();
            }
            self.commit_quantized(
                &candidate.quantized,
                &candidate.gains,
                &candidate.indices,
                rate,
            );
            let config = match bandwidth {
                Bandwidth::Narrowband => 0,
                Bandwidth::Mediumband => 4,
                _ => 8,
            } + u8::from(self.config.frame_size == rate / 50);
            let mut packet = vec![(config << 3) | if channels == 2 { 4 } else { 0 }];
            packet.extend(candidate.payload);
            return Ok(packet);
        }
        Err(Error::Unsupported(format!(
            "SILK excitation cannot fit packet bitrate budget: {last_failure}"
        )))
    }
}

fn sample_rate(rate: SampleRate) -> Result<usize> {
    match rate {
        SampleRate::Hz8000 => Ok(8000),
        SampleRate::Hz12000 => Ok(12000),
        SampleRate::Hz16000 => Ok(16000),
        _ => Err(Error::Unsupported(
            "SILK encoder internal rate must be 8/12/16 kHz".into(),
        )),
    }
}
const fn bandwidth(rate: SampleRate) -> Bandwidth {
    match rate {
        SampleRate::Hz8000 => Bandwidth::Narrowband,
        SampleRate::Hz12000 => Bandwidth::Mediumband,
        _ => Bandwidth::Wideband,
    }
}

fn prediction_error(input: &[i32], lpc: &[i16]) -> i128 {
    input
        .iter()
        .enumerate()
        .map(|(i, &sample)| {
            let prediction = lpc
                .iter()
                .enumerate()
                .take(i)
                .map(|(j, &a)| i64::from(a) * i64::from(input[i - j - 1]))
                .sum::<i64>()
                >> 12;
            let error = i64::from(sample) - prediction;
            i128::from(error) * i128::from(error)
        })
        .sum()
}

// Search the actual quantized LPC filters rather than an unquantized model.
// Coordinate refinement changes every NLSF residual and recomputes stability-
// limited coefficients, so encoder synthesis uses exactly the transmitted model.
#[cfg(test)]
fn analyze(input: &[i32], bandwidth: Bandwidth) -> Result<Model> {
    analyze_rate(input, bandwidth, 48000)
}

fn analyze_rate(input: &[i32], bandwidth: Bandwidth, bitrate: u32) -> Result<Model> {
    // At sparse excitation rates, prefer broad, inexpensive spectral models
    // rather than almost cancelling harmonics with an extremely sharp LPC.
    // Strong adjacent-sample correlation indicates that low-frequency
    // prediction is valuable. Preemphasizing that component before LPC
    // search wastes scarce excitation bits on a signal the predictor could
    // represent; retain preemphasis only for less correlated spectra.
    let energy = input
        .iter()
        .map(|&x| i128::from(x) * i128::from(x))
        .sum::<i128>();
    let correlation = input
        .windows(2)
        .map(|pair| i128::from(pair[0]) * i128::from(pair[1]))
        .sum::<i128>();
    let spectral = if bitrate < 32000 && correlation * 5 < energy * 4 {
        let mut previous = 0_i32;
        input
            .iter()
            .map(|&x| {
                let value = x - previous * 3 / 4;
                previous = x;
                value
            })
            .collect::<Vec<_>>()
    } else {
        input.to_vec()
    };
    let input = spectral.as_slice();
    let order = if bandwidth == Bandwidth::Wideband {
        16
    } else {
        10
    };
    let mut best = None;
    let mut best_error = i128::MAX;
    for stage1 in 0..32 {
        let residual = vec![0; order];
        let lsf = SilkDecoder::reconstruct_lsf(stage1, &residual, bandwidth)?;
        let lpc = SilkDecoder::limit_lpc_coefficients(&lsf, bandwidth)?;
        let error = prediction_error(input, &lpc);
        if error < best_error {
            best_error = error;
            best = Some(Model {
                stage1,
                residual,
                lpc,
            });
        }
    }
    let energy = input
        .iter()
        .map(|&x| i128::from(x) * i128::from(x))
        .sum::<i128>()
        .max(1);
    let rate_weight = if bitrate < 24000 { energy / 300 } else { 0 };
    let mut model = best.ok_or_else(|| Error::Unsupported("empty SILK LPC codebook".into()))?;
    for step in [2_i8, 1] {
        for coordinate in 0..order {
            let original = model.residual[coordinate];
            for candidate in [original - step, original + step] {
                if !(-3..=3).contains(&candidate) {
                    continue;
                }
                let mut residual = model.residual.clone();
                residual[coordinate] = candidate;
                let lsf = SilkDecoder::reconstruct_lsf(model.stage1, &residual, bandwidth)?;
                let lpc = SilkDecoder::limit_lpc_coefficients(&lsf, bandwidth)?;
                let error = prediction_error(input, &lpc)
                    + rate_weight
                        * residual
                            .iter()
                            .map(|v| i128::from(v.unsigned_abs()))
                            .sum::<i128>();
                if error < best_error {
                    best_error = error;
                    model.residual = residual;
                    model.lpc = lpc;
                }
            }
        }
    }
    Ok(model)
}

#[derive(Clone, Copy)]
struct QuantizerInput<'a> {
    input: &'a [i32],
    lpc: &'a [i16],
    history: &'a [i32; 16],
    old_gain: i32,
    gain: i32,
    pitch: Option<&'a Pitch>,
    previous_pcm: &'a [i16],
    rate: usize,
    bitrate: u32,
    rate_pressure: u32,
    optimize_pulse_cost: bool,
}

fn quantize(context: QuantizerInput<'_>) -> (Vec<i16>, [i32; 16], Vec<i16>) {
    let QuantizerInput {
        input,
        lpc,
        history,
        old_gain,
        gain,
        pitch,
        previous_pcm,
        rate,
        bitrate,
        rate_pressure,
        optimize_pulse_cost,
    } = context;
    let mut history = *history;
    if old_gain != gain {
        let ratio = prediction::gain_ratio(old_gain, gain);
        for value in &mut history {
            *value = ((i64::from(*value) * i64::from(ratio)) >> 16) as i32;
        }
    }
    let mut ltp = pitch
        .map(|p| prediction::rewhiten(previous_pcm, lpc, gain, p.lag, rate / 50))
        .unwrap_or_default();
    let mut seed = 0_u32;
    let mut pulses = Vec::with_capacity(input.len().next_multiple_of(16));
    let mut pcm = Vec::with_capacity(input.len());
    let mut error = 0_i32;
    // First-order error feedback shapes quantization noise toward high
    // frequencies. Keep the decoder state separate from the shaping state.
    let shaping = if bitrate < 24000 { 24576_i64 } else { 8192_i64 };
    let offset = if pitch.is_some() { 512 } else { 1600 };
    for (i, &sample) in input.iter().enumerate() {
        let prediction = ((lpc.len() / 2) as i32
            + lpc
                .iter()
                .enumerate()
                .map(|(j, &a)| ((i64::from(history[15 - j]) * i64::from(a)) >> 16) as i32)
                .sum::<i32>())
        .saturating_mul(16);
        let long_prediction = pitch.map_or(0, |pitch| {
            let filter = pitch.coefficients(i / (rate / 200));
            let end = ltp.len();
            let mut sum = 2_i32;
            for (j, &a) in filter.iter().enumerate() {
                sum = sum.wrapping_add(prediction::wb(
                    ltp[end - pitch.lag + 2 - j],
                    i32::from(a) << 7,
                ));
            }
            sum.wrapping_shl(1)
        });
        let target = (i64::from(sample) * (1_i64 << 30) / i64::from(gain))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        seed = seed.wrapping_mul(196_314_165).wrapping_add(907_633_515);
        let sign = if seed & 0x8000_0000 == 0 {
            1_i64
        } else {
            -1_i64
        };
        let shaped = i64::from(target) - i64::from(prediction) - i64::from(long_prediction)
            + ((i64::from(error) * shaping) >> 16);
        let residual = shaped * sign - i64::from(offset);
        let nearest = ((residual + 8192) >> 14).clamp(-126, 126) as i16;
        // Compare adjacent reconstruction levels including the reference
        // quantization-level adjustment and a sparse-pulse rate penalty.
        let mut best = (i128::MAX, 0_i16, 0_i32);
        let lambda = if bitrate < 24000 { 1800_i128 } else { 300_i128 };
        let lambda = lambda << rate_pressure;
        // The L1 pulse-rate cost shifts the optimum toward zero by lambda/2
        // in excitation units. Searching only the unpenalized nearest level
        // and zero misses intermediate, lower-cost nonzero pulses.
        let shrinkage = (lambda / 2) as i64;
        let adjusted = residual.signum() * (residual.abs() - shrinkage).max(0);
        let penalized = ((adjusted + 8192) >> 14).clamp(-126, 126) as i16;
        let extra = (penalized.saturating_sub(1)..=penalized.saturating_add(1))
            .filter(|_| optimize_pulse_cost);
        for pulse in (nearest.saturating_sub(1)..=nearest.saturating_add(1))
            .chain(std::iter::once(0))
            .chain(extra)
        {
            let level = i32::from(pulse) * 16384 - i32::from(pulse.signum()) * 1280 + offset;
            let excitation = (i64::from(level) * sign) as i32;
            let difference = shaped - i64::from(excitation);
            let cost = i128::from(difference) * i128::from(difference)
                + lambda * 16384 * i128::from(pulse.unsigned_abs());
            if cost < best.0 {
                best = (cost, pulse, excitation);
            }
        }
        let (_, pulse, excitation) = best;
        seed = seed.wrapping_add(pulse as u32);
        let residual = excitation.wrapping_add(long_prediction);
        let reconstructed = prediction.saturating_add(residual);
        error = target.saturating_sub(reconstructed).clamp(-32768, 32768);
        if pitch.is_some() {
            ltp.push(residual.wrapping_shl(1));
        }
        history.copy_within(1.., 0);
        history[15] = reconstructed;
        let product = ((i64::from(reconstructed) * i64::from(gain >> 6)) >> 16) as i32;
        pcm.push((((product >> 7) + 1) >> 1).clamp(-32768, 32767) as i16);
        pulses.push(pulse);
    }
    pulses.resize(input.len().next_multiple_of(16), 0);
    (pulses, history, pcm)
}

#[derive(Clone, Copy)]
struct FrameParameters<'a> {
    model: &'a Model,
    gain: i32,
    previous_gain: i32,
    long: bool,
    bandwidth: Bandwidth,
    pitch: Option<&'a Pitch>,
    rate: usize,
}

fn encode_parameters(enc: &mut RangeEncoder, parameters: FrameParameters<'_>) -> Result<()> {
    use silk::constants::{
        FRAME_TYPE_PDF_ACTIVE, GAIN_PDF_DELTA, GAIN_PDF_LSB, GAIN_PDF_UNVOICED, GAIN_PDF_VOICED,
    };

    let FrameParameters {
        model,
        gain,
        previous_gain,
        long,
        bandwidth,
        pitch,
        rate,
    } = parameters;
    enc.ec_enc_icdf(
        if pitch.is_some() { 2 } else { 0 },
        FRAME_TYPE_PDF_ACTIVE,
        8,
    )
    .map_err(entropy)?;
    let coded = gain.max(previous_gain - 16);
    enc.ec_enc_icdf(
        (coded >> 3) as usize,
        if pitch.is_some() {
            GAIN_PDF_VOICED
        } else {
            GAIN_PDF_UNVOICED
        },
        8,
    )
    .map_err(entropy)?;
    enc.ec_enc_icdf((coded & 7) as usize, GAIN_PDF_LSB, 8)
        .map_err(entropy)?;
    for _ in 1..if long { 4 } else { 2 } {
        enc.ec_enc_icdf(4, GAIN_PDF_DELTA, 8).map_err(entropy)?;
    }
    enc.ec_enc_icdf(
        usize::from(model.stage1),
        match (bandwidth == Bandwidth::Wideband, pitch.is_some()) {
            (true, true) => silk::LSF_STAGE1_PDF_WB_VOICED,
            (false, true) => silk::LSF_STAGE1_PDF_NB_MB_VOICED,
            (true, false) => silk::LSF_STAGE1_PDF_WB_INACTIVE,
            (false, false) => silk::LSF_STAGE1_PDF_NB_MB_INACTIVE,
        },
        8,
    )
    .map_err(entropy)?;
    let codebooks = if bandwidth == Bandwidth::Wideband {
        &silk::LSF_CB_SELECT_WB[usize::from(model.stage1)][..]
    } else {
        &silk::LSF_CB_SELECT_NB[usize::from(model.stage1)][..]
    };
    for (&residual, &book) in model.residual.iter().zip(codebooks) {
        let pdf = SilkDecoder::get_lsf_stage2_pdf(book, bandwidth)?;
        enc.ec_enc_icdf((residual + 4) as usize, pdf, 8)
            .map_err(entropy)?;
    }
    if long {
        enc.ec_enc_icdf(4, &[243, 221, 192, 181, 0], 8)
            .map_err(entropy)?;
    }
    if let Some(pitch) = pitch {
        let low = rate / 2000;
        let index = pitch.lag - rate / 500;
        enc.ec_enc_icdf(index / low, silk::LTP_LAG_HIGH_PDF, 8)
            .map_err(entropy)?;
        let pdf = match rate {
            8000 => silk::LTP_LAG_LOW_PDF_NB,
            12000 => silk::LTP_LAG_LOW_PDF_MB,
            _ => silk::LTP_LAG_LOW_PDF_WB,
        };
        enc.ec_enc_icdf(index % low, pdf, 8).map_err(entropy)?;
        let contour = match (rate == 8000, long) {
            (true, false) => silk::PITCH_CONTOUR_PDF_NB_10MS,
            (true, true) => silk::PITCH_CONTOUR_PDF_NB_20MS,
            (false, false) => silk::PITCH_CONTOUR_PDF_MBWB_10MS,
            (false, true) => silk::PITCH_CONTOUR_PDF_MBWB_20MS,
        };
        enc.ec_enc_icdf(0, contour, 8).map_err(entropy)?;
        enc.ec_enc_icdf(pitch.periodicity, silk::LTP_PERIODICITY_PDF, 8)
            .map_err(entropy)?;
        let pdf = match pitch.periodicity {
            0 => silk::LTP_FILTER_PDF_0,
            1 => silk::LTP_FILTER_PDF_1,
            _ => silk::LTP_FILTER_PDF_2,
        };
        for &index in &pitch.filters {
            enc.ec_enc_icdf(index, pdf, 8).map_err(entropy)?;
        }
        enc.ec_enc_icdf(0, silk::LTP_SCALING_PDF, 8)
            .map_err(entropy)?;
    }
    enc.ec_enc_icdf(0, &[192, 128, 64, 0], 8).map_err(entropy)?; // LCG seed.
    Ok(())
}

fn pulse_rate_level(pulses: &[i16], voiced: bool) -> usize {
    // Select the actual least-cost pulse-count model, as encode_pulses.c does.
    let levels = [
        silk::PULSE_COUNT_PDF_LEVEL_0,
        silk::PULSE_COUNT_PDF_LEVEL_1,
        silk::PULSE_COUNT_PDF_LEVEL_2,
        silk::PULSE_COUNT_PDF_LEVEL_3,
        silk::PULSE_COUNT_PDF_LEVEL_4,
        silk::PULSE_COUNT_PDF_LEVEL_5,
        silk::PULSE_COUNT_PDF_LEVEL_6,
        silk::PULSE_COUNT_PDF_LEVEL_7,
        silk::PULSE_COUNT_PDF_LEVEL_8,
    ];
    let rate_pdf = if voiced {
        silk::RATE_LEVEL_PDF_VOICED
    } else {
        silk::RATE_LEVEL_PDF_INACTIVE
    };
    let cost = |pdf: &[u8], symbol: usize| {
        let upper = if symbol == 0 {
            256
        } else {
            u32::from(pdf[symbol - 1])
        };
        // Integer log cost with fractional precision; no floating codec math.
        let probability = upper - u32::from(pdf[symbol]);
        let normalized = probability << probability.leading_zeros();
        (probability.leading_zeros() - 23) * 256 - ((normalized >> 23) & 255)
    };
    (0..9)
        .min_by_key(|&level| {
            cost(rate_pdf, level)
                + pulses
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .map(|block| {
                        let count = block
                            .iter()
                            .map(|p| u32::from(p.unsigned_abs()))
                            .sum::<u32>();
                        cost(levels[level], if count > 16 { 17 } else { count as usize })
                    })
                    .sum::<u32>()
        })
        .unwrap_or(8)
}

fn encode_pulses(enc: &mut RangeEncoder, pulses: &[i16], voiced: bool) -> Result<()> {
    let level = pulse_rate_level(pulses, voiced);
    let levels = [
        silk::PULSE_COUNT_PDF_LEVEL_0,
        silk::PULSE_COUNT_PDF_LEVEL_1,
        silk::PULSE_COUNT_PDF_LEVEL_2,
        silk::PULSE_COUNT_PDF_LEVEL_3,
        silk::PULSE_COUNT_PDF_LEVEL_4,
        silk::PULSE_COUNT_PDF_LEVEL_5,
        silk::PULSE_COUNT_PDF_LEVEL_6,
        silk::PULSE_COUNT_PDF_LEVEL_7,
        silk::PULSE_COUNT_PDF_LEVEL_8,
    ];
    let rate_pdf = if voiced {
        silk::RATE_LEVEL_PDF_VOICED
    } else {
        silk::RATE_LEVEL_PDF_INACTIVE
    };
    enc.ec_enc_icdf(level, rate_pdf, 8).map_err(entropy)?;
    let mut shells = Vec::new();
    for block in pulses.as_chunks::<16>().0 {
        let mut magnitudes: Vec<_> = block.iter().map(|x| x.unsigned_abs()).collect();
        let mut shifts = 0;
        while [(2, 8_u16), (4, 10), (8, 12), (16, 16)]
            .iter()
            .any(|&(size, maximum)| {
                magnitudes
                    .chunks_exact(size)
                    .any(|chunk| chunk.iter().sum::<u16>() > maximum)
            })
        {
            for value in &mut magnitudes {
                *value >>= 1;
            }
            shifts += 1;
        }
        let count = magnitudes.iter().sum::<u16>();
        if shifts == 0 {
            enc.ec_enc_icdf(usize::from(count), levels[level], 8)
                .map_err(entropy)?;
        } else {
            enc.ec_enc_icdf(17, levels[level], 8).map_err(entropy)?;
            for _ in 1..shifts {
                enc.ec_enc_icdf(17, silk::PULSE_COUNT_PDF_LEVEL_9, 8)
                    .map_err(entropy)?;
            }
            enc.ec_enc_icdf(usize::from(count), silk::PULSE_COUNT_PDF_LEVEL_9, 8)
                .map_err(entropy)?;
        }
        shells.push((magnitudes, shifts, count));
    }
    for (magnitudes, _, _) in &shells {
        shell(enc, magnitudes)?;
    }
    for (block, (_, shifts, _)) in pulses.as_chunks::<16>().0.iter().zip(&shells) {
        for value in block {
            for shift in (0..*shifts).rev() {
                enc.ec_enc_icdf(
                    usize::from((value.unsigned_abs() >> shift) & 1),
                    silk::EXCITATION_LSB_PDF,
                    8,
                )
                .map_err(entropy)?;
            }
        }
    }
    let pdfs = if voiced {
        [
            silk::SIGN_PDF_VOICED_LOW_0,
            silk::SIGN_PDF_VOICED_LOW_1,
            silk::SIGN_PDF_VOICED_LOW_2,
            silk::SIGN_PDF_VOICED_LOW_3,
            silk::SIGN_PDF_VOICED_LOW_4,
            silk::SIGN_PDF_VOICED_LOW_5,
            silk::SIGN_PDF_VOICED_LOW_6PLUS,
        ]
    } else {
        [
            silk::SIGN_PDF_UNVOICED_LOW_0,
            silk::SIGN_PDF_UNVOICED_LOW_1,
            silk::SIGN_PDF_UNVOICED_LOW_2,
            silk::SIGN_PDF_UNVOICED_LOW_3,
            silk::SIGN_PDF_UNVOICED_LOW_4,
            silk::SIGN_PDF_UNVOICED_LOW_5,
            silk::SIGN_PDF_UNVOICED_LOW_6PLUS,
        ]
    };
    for (block, (_, _, count)) in pulses.as_chunks::<16>().0.iter().zip(&shells) {
        for &value in block {
            if value != 0 {
                enc.ec_enc_icdf(
                    usize::from(value > 0),
                    pdfs[usize::from((*count).min(6))],
                    8,
                )
                .map_err(entropy)?;
            }
        }
    }
    Ok(())
}

fn shell(enc: &mut RangeEncoder, magnitudes: &[u16]) -> Result<()> {
    let total = magnitudes.iter().sum::<u16>();
    if total == 0 || magnitudes.len() == 1 {
        return Ok(());
    }
    let left = magnitudes[..magnitudes.len() / 2].iter().sum::<u16>();
    let pdf = silk::get_pulse_split_pdf(magnitudes.len(), total as u8)
        .ok_or_else(|| Error::Unsupported("invalid SILK shell pulse count".into()))?;
    enc.ec_enc_icdf(usize::from(left), pdf, 8)
        .map_err(entropy)?;
    shell(enc, &magnitudes[..magnitudes.len() / 2])?;
    shell(enc, &magnitudes[magnitudes.len() / 2..])
}
