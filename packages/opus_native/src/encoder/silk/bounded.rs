//! Bounded stereo native-RD search with transactional scoring.
use super::{
    BTreeMap, BTreeSet, Bandwidth, FrameParameters, Model, PacketCandidate, PrefixCandidateScorer,
    QuantizerInput, RangeEncoder, Result, SharedQuantizedFrame, SilkDecoder, SilkEncoder,
    encode_header, encode_parameters, encode_pulses, quantize_levels,
};
#[cfg(test)]
use super::{EncoderConfig, Error, SampleRate};

use std::rc::Rc;

type ProgramVariants = Vec<(SharedQuantizedFrame, Rc<super::SilkProgram>)>;
type Choice = [(i32, u32, bool); 2];
#[cfg(test)]
type ComponentBeam = Vec<(i128, (i32, u32, bool))>;

struct Search<'a> {
    encoder: &'a SilkEncoder,
    inputs: &'a [Vec<i32>],
    models: &'a [Model],
    incoming: &'a RangeEncoder,
    scorer: &'a mut dyn PrefixCandidateScorer,
    shared_cache: &'a mut super::QuantizerCache,
    model_ids: [usize; 2],
    quantized: BTreeMap<(usize, i32, u32, bool), SharedQuantizedFrame>,
    pulse_programs: BTreeMap<(usize, i32, u32, bool), Rc<super::SilkProgram>>,
    // At fixed channel and gain, pressure and shaping affect the entropy
    // program only through the quantized pulses. Keep one program per distinct
    // pulse sequence instead of rebuilding it under each equivalent choice.
    unique_programs: BTreeMap<(usize, i32), ProgramVariants>,
    left_prefixes: BTreeMap<(i32, u32, bool), (RangeEncoder, bool)>,
    #[cfg(test)]
    proposals: Vec<(i128, usize, Choice)>,
    #[cfg(test)]
    scoring_pool: bool,
    visited: BTreeSet<Choice>,
    legal: Vec<(i128, Choice)>,
    feasibility: Vec<(u64, i128, Choice)>,
    best: Option<PacketCandidate>,
    callbacks: usize,
    #[cfg(test)]
    phases: [u128; 4],
    winner: Option<Choice>,
}

impl Search<'_> {
    #[cfg(test)]
    #[allow(clippy::too_many_lines)] // Bounded prototype proposal domains and native shortlist are kept in one audit.
    fn prototype_pool(&mut self) -> Result<()> {
        // Four native evaluations per model, four models per allocation,
        // three allocations per frame. Proposal generation is independently
        // bounded to 64; there is no hidden exhaustive feasibility fallback.
        let rms: Vec<i32> = self
            .inputs
            .iter()
            .map(|input| {
                let energy =
                    input.iter().map(|&x| i128::from(x).pow(2)).sum::<i128>() / input.len() as i128;
                (0..64)
                    .min_by_key(|&index| {
                        let gain = i128::from(SilkDecoder::dequantize_gain(index));
                        (gain * gain / (1_i128 << 32) - energy).abs()
                    })
                    .unwrap()
            })
            .collect();
        // The full-search diagnostic identifies unshaped, independently
        // pressured channels as a missing domain, not a score approximation.
        for offset in [-4, 0, 4] {
            for pressure in [2, 3, 5, 6, 8, 9, 11, 12] {
                for unshaped in [false, true] {
                    for (left, right) in
                        [(offset, offset), (offset, offset + 2), (offset + 2, offset)]
                    {
                        for (mid_pressure, side_pressure) in [
                            (pressure, pressure),
                            ((pressure + 1).min(12), pressure),
                            (pressure, (pressure + 1).min(12)),
                        ] {
                            self.evaluate([
                                (rms[0] + left, mid_pressure, unshaped),
                                (rms[1] + right, side_pressure, unshaped),
                            ])?;
                        }
                    }
                }
            }
        }
        // Preserve predictive-history scale when the current input RMS drops
        // to zero: delayed API-clock target still contains the previous tail.
        let history_rms: Vec<i32> = self
            .encoder
            .pcm_history
            .iter()
            .take(2)
            .map(|history| {
                let energy = history.iter().map(|&x| i128::from(x).pow(2)).sum::<i128>()
                    / history.len().max(1) as i128;
                (0..64)
                    .min_by_key(|&index| {
                        let gain = i128::from(SilkDecoder::dequantize_gain(index));
                        (gain * gain / (1_i128 << 32) - energy).abs()
                    })
                    .unwrap()
            })
            .collect();
        for offset in [-16, -8, 0] {
            for pressure in [3, 6, 9, 12] {
                self.evaluate([
                    (history_rms[0] + offset, pressure, false),
                    (history_rms[1] + offset, pressure, false),
                ])?;
            }
        }
        for offset in [-2, -1, 0, 1, 2] {
            for pressure in [2, 4, 8, 12] {
                for unshaped in [false, true] {
                    self.evaluate([
                        (self.encoder.previous_index[0] + offset, pressure, unshaped),
                        (self.encoder.previous_index[1] + offset, pressure, unshaped),
                    ])?;
                }
            }
        }
        for pressure in [0, 3, 6, 9, 12] {
            self.evaluate([
                (self.encoder.previous_index[0], pressure, true),
                (self.encoder.previous_index[1], pressure, true),
            ])?;
        }
        // Form independent channel beams from already reconstructed proposals.
        // This admits mixed shaping and non-neighbor pressures without a full
        // Cartesian native search. Raw SSE only proposes; native SSE selects.
        let mut beams: [ComponentBeam; 2] = std::array::from_fn(|_| Vec::new());
        for (channel, beam) in beams.iter_mut().enumerate() {
            for (&(c, index, pressure, unshaped), frame) in &self.quantized {
                if c != channel {
                    continue;
                }
                let zeros = vec![0_i16; frame.2.len()];
                let error = if channel == 0 {
                    self.scorer
                        .proposal_score(self.encoder.config, &frame.2, &zeros)?
                } else {
                    self.scorer
                        .proposal_score(self.encoder.config, &zeros, &frame.2)?
                }
                .unwrap_or_else(|| {
                    self.inputs[channel]
                        .iter()
                        .zip(&frame.2)
                        .map(|(&x, &y)| (i128::from(x) - i128::from(y)).pow(2))
                        .sum()
                });
                beam.push((error, (index, pressure, unshaped)));
            }
            beam.sort_unstable();
            let mut diverse = Vec::new();
            for unshaped in [false, true] {
                diverse.extend(
                    beam.iter()
                        .filter(|entry| entry.1.2 == unshaped)
                        .take(4)
                        .copied(),
                );
            }
            *beam = diverse;
        }
        for left in &beams[0] {
            for right in &beams[1] {
                self.evaluate([left.1, right.1])?;
            }
        }
        self.evaluate([(0, 13, false); 2])?;
        self.proposals.sort_by_key(|entry| (entry.0, entry.1));
        self.scoring_pool = true;
        // Seeds are selected in the actual incoming API-clock domain. Shaping
        // diversity is retained in component proposals, not forced over a
        // better projected mixed-channel candidate at the final shortlist.
        let seeds: Vec<_> = self.proposals.iter().take(1).map(|entry| entry.2).collect();
        for choice in seeds {
            self.visited.remove(&choice);
            self.evaluate(choice)?;
        }
        let refinement_start = self.visited.len();
        for step in [32, 16, 8] {
            if let Some(mut choice) = self.winner {
                let difference = choice[0].0 - choice[1].0;
                if difference.abs() < 12 {
                    // Refine the native winner, not the raw-RMS seed. Full
                    // diagnostics found 1/2-index and independent-pressure
                    // winners missing from the coarse component lattice.
                    self.scoring_pool = false;
                    for channel in 0..2 {
                        let coordinate_begin = self.proposals.len();
                        for delta in [-2, -1, 1, 2] {
                            let mut proposal = choice;
                            proposal[channel].0 += delta;
                            self.evaluate(proposal)?;
                        }
                        for delta in [-1_i32, 1] {
                            let mut proposal = choice;
                            proposal[channel].1 =
                                (proposal[channel].1 as i32 + delta).clamp(0, 12) as u32;
                            self.evaluate(proposal)?;
                        }
                        let mut proposal = choice;
                        proposal[channel].2 = !proposal[channel].2;
                        self.evaluate(proposal)?;
                        for delta in [-1, 1] {
                            let mut proposal = choice;
                            proposal[channel].0 += delta;
                            proposal[channel].2 = !proposal[channel].2;
                            self.evaluate(proposal)?;
                        }
                        if let Some(entry) = self.proposals[coordinate_begin..]
                            .iter()
                            .min_by_key(|entry| (entry.0, entry.1))
                        {
                            let current = self
                                .proposals
                                .iter()
                                .find(|entry| entry.2 == choice)
                                .map_or(i128::MAX, |entry| entry.0);
                            if entry.0 < current {
                                choice = entry.2;
                            }
                        }
                    }
                    for left_pressure in [-1_i32, 0, 1] {
                        for right_gain in [-1, 0, 1] {
                            for right_shape in [false, true] {
                                let mut proposal = choice;
                                proposal[0].1 =
                                    (proposal[0].1 as i32 + left_pressure).clamp(0, 12) as u32;
                                proposal[1].0 += right_gain;
                                if right_shape {
                                    proposal[1].2 = !proposal[1].2;
                                }
                                self.evaluate(proposal)?;
                                let mut mirrored = choice;
                                mirrored[1].1 =
                                    (mirrored[1].1 as i32 + left_pressure).clamp(0, 12) as u32;
                                mirrored[0].0 += right_gain;
                                if right_shape {
                                    mirrored[0].2 = !mirrored[0].2;
                                }
                                self.evaluate(mirrored)?;
                            }
                        }
                    }
                    for left_gain in [-1, 0, 1] {
                        for right_gain in [-1, 0, 1] {
                            for left_pressure in [-1_i32, 0, 1] {
                                for right_pressure in [-1_i32, 0, 1] {
                                    let mut proposal = choice;
                                    proposal[0].0 += left_gain;
                                    proposal[1].0 += right_gain;
                                    proposal[0].1 =
                                        (proposal[0].1 as i32 + left_pressure).clamp(0, 12) as u32;
                                    proposal[1].1 =
                                        (proposal[1].1 as i32 + right_pressure).clamp(0, 12) as u32;
                                    self.evaluate(proposal)?;
                                }
                            }
                        }
                    }
                    self.scoring_pool = true;
                    self.proposals.sort_by_key(|entry| (entry.0, entry.1));
                    if let Some(entry) = self
                        .proposals
                        .iter()
                        .find(|entry| !self.legal.iter().any(|legal| legal.1 == entry.2))
                    {
                        choice = entry.2;
                    }
                } else {
                    for (channel, entry) in choice.iter_mut().enumerate() {
                        if (difference > 0 && channel == 0) || (difference < 0 && channel == 1) {
                            entry.0 = (entry.0 - step).max(0);
                        }
                    }
                }
                self.visited.remove(&choice);
                self.evaluate(choice)?;
            }
        }
        eprintln!(
            "POOL_REFINEMENT proposals_before={refinement_start} proposals_after={} native_calls={}",
            self.visited.len(),
            self.callbacks
        );
        Ok(())
    }

    fn anchor(&self, feasibility: bool) -> Choice {
        if feasibility && let Some(entry) = self.feasibility.first() {
            return entry.2;
        }
        self.legal.first().map_or_else(
            || {
                self.feasibility
                    .first()
                    .map_or([(32, 6, false); 2], |entry| entry.2)
            },
            |entry| entry.1,
        )
    }

    fn frames(&mut self, choice: Choice) -> Vec<SharedQuantizedFrame> {
        let config = self.encoder.config;
        let mut frames = Vec::new();
        for (channel, &(index, pressure, unshaped)) in choice.iter().enumerate() {
            let key = (channel, index, pressure, unshaped);
            let shared_key = (self.model_ids[channel], index, pressure, unshaped);
            let frame = self.quantized.entry(key).or_insert_with(|| {
                let reuse = true;
                #[cfg(test)]
                let reuse = reuse && !super::REFERENCE_PAIRS.get();
                if reuse && let Some(frame) = self.shared_cache.bounded_frames.get(&shared_key) {
                    return frame.clone();
                }
                let frame = std::sync::Arc::new(quantize_levels(
                    QuantizerInput {
                        input: &self.inputs[channel],
                        lpc: &self.models[channel].lpc,
                        history: &self.encoder.history[channel],
                        old_gain: self.encoder.gain[channel],
                        gain: SilkDecoder::dequantize_gain(index),
                        pitch: None,
                        previous_pcm: &self.encoder.pcm_history[channel],
                        rate: 16000,
                        bitrate: config.bitrate / 2,
                        rate_pressure: pressure.min(12),
                        optimize_pulse_cost: true,
                        unshaped,
                    },
                    pressure == 13,
                ));
                self.shared_cache
                    .bounded_frames
                    .insert(shared_key, frame.clone());
                frame
            });
            frames.push(frame.clone());
        }
        frames
    }

    #[allow(clippy::too_many_lines)] // Ordered exact proposal, feasibility, replay and scorer phases.
    fn evaluate(&mut self, mut choice: Choice) -> Result<()> {
        for entry in &mut choice {
            entry.0 = entry.0.clamp(0, 63);
            entry.1 = entry.1.min(13);
        }
        if self.visited.len() >= 960 || !self.visited.insert(choice) {
            return Ok(());
        }
        let config = self.encoder.config;
        #[cfg(test)]
        let phase = std::time::Instant::now();
        let frames = self.frames(choice);
        #[cfg(test)]
        {
            self.phases[0] += phase.elapsed().as_nanos();
        }

        let programs: Vec<_> = choice
            .iter()
            .enumerate()
            .map(|(channel, &key)| {
                let key = (channel, key.0, key.1, key.2);
                if let Some(program) = self.pulse_programs.get(&key) {
                    return Ok(program.clone());
                }
                let bucket = self.unique_programs.entry((channel, key.1)).or_default();
                if let Some((_, program)) = bucket
                    .iter()
                    .find(|(frame, _)| frame.0 == frames[channel].0)
                {
                    self.pulse_programs.insert(key, program.clone());
                    return Ok(program.clone());
                }
                let mut program = super::SilkProgram::default();
                encode_parameters(
                    &mut program,
                    FrameParameters {
                        model: &self.models[channel],
                        gain: choice[channel].0,
                        previous_gain: self.encoder.previous_index[channel],
                        long: config.frame_size == 320,
                        bandwidth: Bandwidth::Wideband,
                        pitch: None,
                        rate: 16000,
                    },
                )?;
                encode_pulses(&mut program, &frames[channel].0, false)?;
                let program = Rc::new(program);
                bucket.push((frames[channel].clone(), program.clone()));
                self.pulse_programs.insert(key, program.clone());
                Ok(program)
            })
            .collect::<Result<Vec<_>>>()?;
        let direct = false;
        #[cfg(test)]
        let direct = direct || super::REFERENCE_PAIRS.get();
        let encode = |coder: &mut RangeEncoder| -> Result<()> {
            encode_header(coder, 2)?;
            #[allow(unused_variables)]
            // Channel identity is needed only by direct-path differential tests.
            for (channel, program) in programs.iter().enumerate() {
                #[cfg(test)]
                if direct {
                    encode_parameters(
                        coder,
                        FrameParameters {
                            model: &self.models[channel],
                            gain: choice[channel].0,
                            previous_gain: self.encoder.previous_index[channel],
                            long: config.frame_size == 320,
                            bandwidth: Bandwidth::Wideband,
                            pitch: None,
                            rate: 16000,
                        },
                    )?;
                    encode_pulses(coder, &frames[channel].0, false)?;
                    continue;
                }
                program.replay(coder)?;
            }
            Ok(())
        };
        let budget = (config.bitrate as usize * config.frame_size / 128_000).saturating_sub(1) * 8;
        #[cfg(test)]
        let phase = std::time::Instant::now();
        let (coder, valid) = if direct || self.incoming.storage() > 1275 {
            let mut coder = self.incoming.clone();
            let valid = encode(&mut coder).is_ok();
            (coder, valid)
        } else {
            // The incoming state is fixed for this search. Many proposals change
            // only the right channel; resume from the exact left-channel state.
            let (prefix, left_valid) = self.left_prefixes.entry(choice[0]).or_insert_with(|| {
                let mut coder = self.incoming.clone();
                let valid =
                    encode_header(&mut coder, 2).is_ok() && programs[0].replay(&mut coder).is_ok();
                (coder, valid)
            });
            let mut coder = prefix.clone();
            let valid = *left_valid && programs[1].replay(&mut coder).is_ok();
            (coder, valid)
        };
        #[cfg(test)]
        {
            self.phases[1] += phase.elapsed().as_nanos();
        }
        let bits = coder.tell().saturating_sub(self.incoming.tell());
        // Legal candidates use the decoded score. Only compute the input-domain
        // error for an infeasible candidate (or the diagnostic proposal pool).
        let raw = || -> i128 {
            self.inputs
                .iter()
                .zip(&frames)
                .map(|(input, frame)| {
                    input
                        .iter()
                        .zip(&frame.2)
                        .map(|(&x, &y)| (i128::from(x) - i128::from(y)).pow(2))
                        .sum::<i128>()
                })
                .sum()
        };
        if !valid || bits as usize > budget {
            let excess = if valid {
                bits.saturating_sub(budget as u64)
            } else {
                u64::MAX
            };
            // Error only breaks ties after excess bits. A strictly worse
            // excess cannot survive truncation regardless of reconstruction.
            if self
                .feasibility
                .get(1)
                .is_some_and(|entry| excess > entry.0)
            {
                return Ok(());
            }
            self.feasibility.push((excess, raw(), choice));
            self.feasibility.sort_unstable();
            self.feasibility.truncate(2);
            return Ok(());
        }
        #[cfg(test)]
        if super::super::hybrid::POOL48.get() && !self.scoring_pool {
            let proposed = self
                .scorer
                .proposal_score(config, &frames[0].2, &frames[1].2)?
                .unwrap_or_else(raw);
            self.proposals
                .push((proposed, self.proposals.len(), choice));
            return Ok(());
        }
        #[cfg(test)]
        if super::super::hybrid::POOL48.get() {
            let calls = super::super::hybrid::POOL48_EVALUATIONS.get();
            assert!(calls < 48, "frame-global native evaluation cap");
            super::super::hybrid::POOL48_EVALUATIONS.set(calls + 1);
        }
        #[cfg(test)]
        let phase = std::time::Instant::now();
        let replay = super::replay::encode(encode)?;
        #[cfg(test)]
        {
            self.phases[2] += phase.elapsed().as_nanos();
        }
        #[cfg(test)]
        let phase = std::time::Instant::now();
        let score = self.scorer.score(config, &replay)?;
        #[cfg(test)]
        if super::super::hybrid::POOL48.get() && super::super::hybrid::POOL_DIAGNOSE.get() > 0 {
            let projected = self
                .scorer
                .proposal_score(config, &frames[0].2, &frames[1].2)?;
            let raw = raw();
            eprintln!(
                "POOL_NATIVE choice={choice:?} raw={raw} projected={projected:?} actual={score} bits={bits} previous={:?}",
                self.encoder.previous_index
            );
        }
        #[cfg(test)]
        {
            self.phases[3] += phase.elapsed().as_nanos();
        }
        self.callbacks += 1;
        self.legal.push((score, choice));
        self.legal.sort_unstable();
        self.legal.truncate(4);
        if self.best.as_ref().is_none_or(|old| score < old.distortion) {
            self.winner = Some(choice);
            self.best = Some(PacketCandidate {
                distortion: score,
                payload: Vec::new(),
                prefix: Some(coder),
                indices: choice.iter().map(|entry| entry.0).collect(),
                gains: choice
                    .iter()
                    .map(|entry| SilkDecoder::dequantize_gain(entry.0))
                    .collect(),
                quantized: frames,
            });
        }
        Ok(())
    }

    // Drive a common log-gain correction with actual range-coded bit usage.
    // Preserve the channel energy ratio rather than searching a gain-pair grid.
    fn rate_control(&mut self) -> Result<()> {
        let centers: [i32; 2] = std::array::from_fn(|c| {
            let x = &self.inputs[c];
            let energy = x.iter().map(|&v| i64::from(v).pow(2)).sum::<i64>() / x.len() as i64;
            (0..64)
                .min_by_key(|&i| {
                    let g = i64::from(SilkDecoder::dequantize_gain(i));
                    (((g * g) >> 32) - energy).abs()
                })
                .unwrap()
        });
        for &pressure in if self.encoder.config.frame_size == 160 {
            &[2, 4, 8][..]
        } else {
            &[3, 5][..]
        } {
            let (mut lo, mut hi) = (-centers[0].max(centers[1]), 63 - centers[0].min(centers[1]));
            while lo <= hi {
                let offset = lo + (hi - lo) / 2;
                let choice = centers.map(|g| ((g + offset).clamp(0, 63), pressure, true));
                let before = self.callbacks;
                self.evaluate(choice)?;
                if self.callbacks > before {
                    hi = offset - 1;
                } else {
                    lo = offset + 1;
                }
            }
        }
        let mute = [(0, 13, false); 2];
        self.evaluate(mute)?;
        // A mute winner can indicate that the gain-only loop skipped sparse,
        // affordable excitation. Compare high pulse penalties at independently
        // scaled gains before committing to a near-silent reconstruction.
        if self.winner == Some(mute) && centers != [0, 0] {
            for pressure in [10, 11, 12] {
                for offset in [-32, -24, -16, -8, 0] {
                    for unshaped in [false, true] {
                        self.evaluate(std::array::from_fn(|c| {
                            ((centers[c] + offset).clamp(0, 63), pressure, unshaped)
                        }))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn seeds(&mut self) -> Result<()> {
        let rms: Vec<_> = self
            .inputs
            .iter()
            .map(|input| {
                let energy =
                    input.iter().map(|&x| i128::from(x).pow(2)).sum::<i128>() / input.len() as i128;
                (0..64)
                    .min_by_key(|&index| {
                        let gain = i128::from(SilkDecoder::dequantize_gain(index));
                        (gain * gain / (1_i128 << 32) - energy).abs()
                    })
                    .unwrap_or(32)
            })
            .collect();
        // With Q14 excitation and Q16 synthesis gain, unit excitation RMS
        // requires gain = PCM_RMS * 65536. Half-unit excitation requires
        // twice that gain. Project both scales onto the actual gain table,
        // rather than adding a fixture-specific gain-index offset.
        let doubled: Vec<_> = rms
            .iter()
            .map(|&index| {
                let target = i64::from(SilkDecoder::dequantize_gain(index)) * 2;
                (0..64)
                    .min_by_key(|&i| (i64::from(SilkDecoder::dequantize_gain(i)) - target).abs())
                    .unwrap_or(63)
            })
            .collect();
        for left in [rms[0], doubled[0]] {
            for right in [rms[1], doubled[1]] {
                // Cover the pulse penalty's 12-bit exponent range in quarters.
                for pressure in [0, 4, 8, 12] {
                    for left_shape in [false, true] {
                        for right_shape in [false, true] {
                            self.evaluate([
                                (left, pressure, left_shape),
                                (right, pressure, right_shape),
                            ])?;
                        }
                    }
                }
            }
        }
        for pressure in [0, 4, 8, 12] {
            for left_shape in [false, true] {
                for right_shape in [false, true] {
                    self.evaluate([
                        (self.encoder.previous_index[0], pressure, left_shape),
                        (self.encoder.previous_index[1], pressure, right_shape),
                    ])?;
                }
            }
        }
        #[cfg(test)]
        eprintln!(
            "SILK_BOUNDED_SEEDS rms={rms:?} doubled={doubled:?} previous={:?}",
            self.encoder.previous_index
        );
        // Zero symbols minimize pulse syntax, but mandatory offset excitation
        // still needs decoder-matched synthesis at each proposed gain.
        // Probe both ends of the legal gain lattice and both signal-derived
        // scales. Offset-only reconstruction is gain-dependent even at zero.
        for gain in [0, rms[0], doubled[0], 63] {
            for side_gain in [0, rms[1], doubled[1], 63] {
                for unshaped in [false, true] {
                    self.evaluate([(gain, 13, unshaped), (side_gain, 13, unshaped)])?;
                }
            }
        }
        Ok(())
    }

    fn beam(&self) -> Vec<Choice> {
        let mut beam: Vec<_> = self
            .legal
            .iter()
            .take(2)
            .map(|entry| entry.1)
            .chain(self.feasibility.iter().map(|entry| entry.2))
            .chain(self.legal.iter().skip(2).map(|entry| entry.1))
            .take(4)
            .collect();
        beam.dedup();
        if beam.is_empty() {
            beam.push(self.anchor(false));
        }
        beam
    }

    fn refine(&mut self) -> Result<()> {
        // Refresh the four-state beam between channels so right-channel
        // trials include left-channel improvements. Both shaping choices are
        // tested for every gain/penalty neighbor: 2*4*9*5*2 = 720 proposals.
        for channel in 0..2 {
            for anchor in self.beam() {
                for gain in [-8, -4, -2, -1, 0, 1, 2, 4, 8] {
                    for pressure in [-2_i32, -1, 0, 1, 2] {
                        for unshaped in [false, true] {
                            let mut choice = anchor;
                            choice[channel] = (
                                anchor[channel].0 + gain,
                                (anchor[channel].1.min(12) as i32 + pressure).clamp(0, 12) as u32,
                                unshaped,
                            );
                            self.evaluate(choice)?;
                        }
                    }
                }
            }
        }
        // 4 states * 4 * 4 * 2 = 128 joint proposals.
        for anchor in self.beam() {
            for left in [-8, -2, 2, 8] {
                for right in [-8, -2, 2, 8] {
                    for pressure in [-2_i32, 2] {
                        self.evaluate([
                            (
                                anchor[0].0 + left,
                                (anchor[0].1.min(12) as i32 + pressure).clamp(0, 12) as u32,
                                anchor[0].2,
                            ),
                            (
                                anchor[1].0 + right,
                                (anchor[1].1.min(12) as i32 + pressure).clamp(0, 12) as u32,
                                anchor[1].2,
                            ),
                        ])?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl SilkEncoder {
    #[allow(clippy::too_many_lines)] // Same-state reference instrumentation surrounds the fixed production search.
    pub(super) fn bounded_stereo_search(
        &self,
        inputs: &[Vec<i32>],
        models: &[Model],
        incoming: &RangeEncoder,
        scorer: &mut dyn PrefixCandidateScorer,
        shared_cache: &mut super::QuantizerCache,
    ) -> Result<Option<PacketCandidate>> {
        #[cfg(test)]
        let skip_diagnostic = super::super::hybrid::POOL_DIAGNOSE_SKIP.get();
        #[cfg(test)]
        if skip_diagnostic > 0 {
            super::super::hybrid::POOL_DIAGNOSE_SKIP.set(skip_diagnostic - 1);
        }
        #[cfg(test)]
        let reference = if super::super::hybrid::POOL48.get()
            && skip_diagnostic == 0
            && super::super::hybrid::POOL_DIAGNOSE.get() > 0
        {
            super::super::hybrid::POOL_DIAGNOSE.set(super::super::hybrid::POOL_DIAGNOSE.get() - 1);
            let mut fork = scorer.diagnostic_fork().expect("native diagnostic scorer");
            super::super::hybrid::POOL48.set(false);
            let full = self.bounded_stereo_search(
                inputs,
                models,
                incoming,
                fork.as_mut(),
                &mut super::QuantizerCache::default(),
            );
            super::super::hybrid::POOL48.set(true);
            full?
        } else {
            None
        };
        #[cfg(test)]
        let started = std::time::Instant::now();
        let model_ids = std::array::from_fn(|channel| {
            shared_cache.model_id(channel, &models[channel].lpc, None)
        });
        let mut search = Search {
            encoder: self,
            inputs,
            models,
            incoming,
            scorer,
            shared_cache,
            model_ids,
            quantized: BTreeMap::new(),
            pulse_programs: BTreeMap::new(),
            unique_programs: BTreeMap::new(),
            left_prefixes: BTreeMap::new(),
            #[cfg(test)]
            proposals: Vec::new(),
            #[cfg(test)]
            scoring_pool: false,
            visited: BTreeSet::new(),
            legal: Vec::new(),
            feasibility: Vec::new(),
            best: None,
            callbacks: 0,
            #[cfg(test)]
            phases: [0; 4],
            winner: None,
        };
        #[cfg(test)]
        if super::super::hybrid::POOL48.get() {
            search.prototype_pool()?;
            if let Some(reference) = reference {
                eprintln!(
                    "POOL_COMPARE bitrate={} size={} model_stages={:?} full_indices={:?} full_score={} pool_indices={:?} pool_score={:?}",
                    self.config.bitrate,
                    self.config.frame_size,
                    models.iter().map(|m| m.stage1).collect::<Vec<_>>(),
                    reference.indices,
                    reference.distortion,
                    search.best.as_ref().map(|p| &p.indices),
                    search.best.as_ref().map(|p| p.distortion)
                );
                for (rank, (raw, _, choice)) in search.proposals.iter().enumerate() {
                    eprintln!("POOL_PROPOSAL rank={rank} raw={raw} choice={choice:?}");
                }
            }
            return search
                .best
                .ok_or_else(|| {
                    Error::Unsupported(
                        "pool48 has no legal proposal; no exhaustive fallback".into(),
                    )
                })
                .map(Some);
        }
        if self.prefix_search == super::PrefixSearch::History {
            // The incoming gain indices follow the committed quantized predictor.
            // Test a few correlated steps, not an independent mid/side grid.
            for (delta, pressure) in [(0, 4), (-4, 8), (4, 4), (8, 2), (-8, 10)] {
                search.evaluate(
                    self.previous_index
                        .map(|g| ((g + delta).clamp(0, 63), pressure, true)),
                )?;
            }
            search.rate_control()?;
        } else if self.prefix_search == super::PrefixSearch::Rate || self.config.frame_size == 320 {
            search.rate_control()?;
            if search.best.is_none() {
                return Err(crate::Error::Unsupported(
                    "no legal rate-controlled stereo prefix".into(),
                ));
            }
        } else if self.prefix_search == super::PrefixSearch::QuietRefine {
            search.rate_control()?;
            for channel in 0..2 {
                let anchor = search.anchor(false);
                for gain in [-4, 0, 4] {
                    for pressure in [0, 2, 4] {
                        for unshaped in [false, true] {
                            let mut trial = anchor;
                            trial[channel] =
                                ((anchor[channel].0 + gain).clamp(0, 63), pressure, unshaped);
                            search.evaluate(trial)?;
                        }
                    }
                }
            }
        } else {
            search.seeds()?;
            search.refine()?;
        }
        #[cfg(test)]
        eprintln!(
            "SILK_BOUNDED trials={} quantizations={} callbacks={} fallback={} elapsed_us={}",
            search.visited.len(),
            search.quantized.len(),
            search.callbacks,
            search.best.is_none(),
            started.elapsed().as_micros()
        );
        #[cfg(test)]
        eprintln!(
            "SILK_BOUNDED_PHASE quantize_ns={} shared_entropy_ns={} replay_ns={} callback_ns={}",
            search.phases[0], search.phases[1], search.phases[2], search.phases[3]
        );
        #[cfg(test)]
        eprintln!(
            "SILK_BOUNDED_WIN score={:?} choice={:?}",
            search.best.as_ref().map(|entry| entry.distortion),
            search.winner
        );
        Ok(search.best)
    }
}

#[test]
fn bounded_stereo_native_transaction() {
    use crate::encoder::hybrid::{NativePrefixScore, NativeScoreCache};
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Stereo,
        frame_size: 160,
        bitrate: 24000,
    };
    let input: Vec<_> = (0..160)
        .flat_map(|i| {
            let x = (std::f32::consts::TAU * 143.0 * i as f32 / 16000.0).sin() * 0.1;
            [x, -0.7 * x]
        })
        .collect();
    let mut encoder = SilkEncoder::new(config).unwrap();
    encoder.bounded_prefix_search = true;
    let mut score = NativePrefixScore {
        incoming: crate::Decoder::new(SampleRate::Hz48000, config.channels).unwrap(),
        target: vec![0; 960],
        best: None,
        cache: NativeScoreCache::default(),
    };
    let mut range = RangeEncoder::new(256).unwrap();
    let initial = range.tell();
    encoder
        .encode_prefix_scored(&input, &mut range, &mut score)
        .unwrap();
    assert!(range.tell() - initial <= 232);
    score.verify(&range.finish().unwrap()).unwrap();
    eprintln!("BOUNDED_NATIVE score={}", score.best.unwrap().0);
}

#[test]
fn bounded_scorer_error_is_atomic() {
    struct Reject;
    impl PrefixCandidateScorer for Reject {
        fn score(&mut self, _: EncoderConfig, _: &[u8]) -> Result<i128> {
            Err(Error::Unsupported("bounded sentinel".into()))
        }
    }
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Stereo,
        frame_size: 160,
        bitrate: 24000,
    };
    let mut encoder = SilkEncoder::new(config).unwrap();
    encoder.bounded_prefix_search = true;
    let original = encoder.clone();
    let mut range = RangeEncoder::new(256).unwrap();
    let before = range.clone().finish().unwrap();
    let result = encoder.encode_prefix_scored(&[0.0; 320], &mut range, &mut Reject);
    assert!(matches!(result, Err(Error::Unsupported(message)) if message == "bounded sentinel"));
    assert_eq!(range.finish().unwrap(), before);
    assert_eq!(encoder.history, original.history);
    assert_eq!(encoder.gain, original.gain);
    assert_eq!(encoder.previous_index, original.previous_index);
    assert_eq!(encoder.pcm_history, original.pcm_history);
}

#[test]
#[cfg(feature = "hybrid")]
#[ignore = "bounded search actual reviewed music24 stereo10 qualification"]
fn bounded_reviewed_music_layout() {
    use crate::encoder::{Encoder, EncoderControl, EncoderMode};
    let directory = std::env::var("MOOSICBOX_OPUS_QUALIFICATION_MEDIA").unwrap();
    let script = format!("{}/test_runner/src", env!("CARGO_MANIFEST_DIR"));
    let output = std::process::Command::new("python3").env("PYTHONDONTWRITEBYTECODE", "1")
        .args(["-c", "import sys,wave;sys.path.insert(0,sys.argv[1]);from qualification_inputs import verify_inputs;_,p=verify_inputs(sys.argv[2]);w=wave.open(str(p['music']),'rb');sys.stdout.buffer.write(w.readframes(w.getnframes()))", &script, &directory]).output().unwrap();
    assert!(output.status.success());
    let media: Vec<_> = output
        .stdout
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&x| f32::from(i16::from_le_bytes(x)) / 32768.0)
        .collect();
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz24000,
        channels: crate::Channels::Stereo,
        frame_size: 240,
        bitrate: 64000,
    };
    let mut encoder = Encoder::new_auto_layout(config, true).unwrap();
    encoder.silk.as_mut().unwrap().bounded_prefix_search =
        std::env::var_os("MOOSICBOX_OPUS_TEST_EXHAUSTIVE").is_none();
    let delay = encoder.lookahead();
    let mut packets = Vec::new();
    let mut source = Vec::new();
    let started = std::time::Instant::now();
    for frame in 0..32 {
        let control = EncoderControl {
            mode: if (4..12).contains(&frame) {
                EncoderMode::Hybrid
            } else if (16..24).contains(&frame) {
                EncoderMode::Auto
            } else {
                EncoderMode::Celt
            },
            bitrate: if (4..12).contains(&frame) {
                32000
            } else {
                64000
            },
            bandwidth: Bandwidth::Fullband,
        };
        let input: Vec<_> = media[frame * 960..(frame + 1) * 960]
            .as_chunks::<2>()
            .0
            .iter()
            .step_by(2)
            .flat_map(|pair| pair.iter().copied())
            .collect();
        encoder.set_control(control).unwrap();
        let packet = encoder.encode(&input).unwrap();
        assert!(packet.len() <= control.bitrate as usize * 480 / 384_000);
        packets.push(packet);
        source.extend(input);
    }
    packets.extend(encoder.finish().unwrap());
    let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
    let mut output_pcm = Vec::new();
    for packet in &packets {
        let mut pcm = vec![0; 480];
        assert_eq!(decoder.decode(Some(packet), &mut pcm, false).unwrap(), 240);
        output_pcm.extend(pcm);
    }
    let mut failures = Vec::new();
    for (begin, end) in [(4, 6), (6, 12), (12, 14), (16, 18), (18, 24), (24, 26)] {
        let range = begin * 480..end * 480;
        let signal: f64 = source[range.clone()]
            .iter()
            .map(|&x| f64::from(x).powi(2))
            .sum();
        let error: f64 = source[range.clone()]
            .iter()
            .zip(&output_pcm[delay * 2 + range.start..delay * 2 + range.end])
            .map(|(&x, &y)| (f64::from(x) - f64::from(y) / 32768.0).powi(2))
            .sum();
        let snr = 10.0 * (signal / error).log10();
        #[cfg(test)]
        eprintln!("BOUNDED_MUSIC boundary={begin} snr={snr}");
        if snr <= 3.0 {
            failures.push((begin, snr));
        }
    }
    eprintln!("BOUNDED_MUSIC total_us={}", started.elapsed().as_micros());
    assert!(
        failures.is_empty(),
        "bounded quality failures: {failures:?}"
    );
}

// Replay programs and direct pulse entropy must visit/select the same
// candidates from identical predictor/range state.
#[cfg(test)]
#[test]
fn bounded_program_search_is_direct_exact() {
    struct Raw;
    impl PrefixCandidateScorer for Raw {
        fn score(&mut self, _: EncoderConfig, replay: &[u8]) -> Result<i128> {
            // Stable observational objective; all syntax bytes contribute.
            Ok(replay
                .iter()
                .enumerate()
                .map(|(i, &x)| (i as i128 + 1) * i128::from(x))
                .sum())
        }
    }
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Stereo,
        frame_size: 160,
        bitrate: 24000,
    };
    let mut a = SilkEncoder::new(config).unwrap();
    let mut b = a.clone();
    let input: Vec<f32> = (0..320)
        .map(|i| ((i * 113 % 997) as f32 - 498.0) / 32768.0)
        .collect();
    let mut x = RangeEncoder::new(256).unwrap();
    let mut y = x.clone();
    a.encode_prefix_scored(&input, &mut x, &mut Raw).unwrap();
    super::REFERENCE_PAIRS.set(true);
    let result = b.encode_prefix_scored(&input, &mut y, &mut Raw);
    super::REFERENCE_PAIRS.set(false);
    result.unwrap();
    assert_eq!(x.finish().unwrap(), y.finish().unwrap());
    assert_eq!(a.history, b.history);
    assert_eq!(a.gain, b.gain);
    assert_eq!(a.previous_index, b.previous_index);
    assert_eq!(a.pcm_history, b.pcm_history);
}
