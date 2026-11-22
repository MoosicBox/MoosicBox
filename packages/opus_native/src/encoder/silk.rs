//! SILK encoder with integer LPC/pitch analysis and noise-shaped excitation.
#[path = "silk_prediction.rs"]
mod prediction;
#[path = "silk_pulse.rs"]
mod pulse;
#[path = "silk/replay.rs"]
mod replay;
#[cfg(all(test, feature = "hybrid"))]
#[path = "silk/replay_tests.rs"]
mod replay_tests;
use super::{EncoderConfig, entropy};
use crate::range::RangeEncoder;
use crate::silk::{self, SilkDecoder};
use crate::{Bandwidth, Error, Result, SampleRate};
use prediction::Pitch;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
#[path = "silk/bounded.rs"]
mod bounded;
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
#[cfg(test)]
#[path = "silk_scored.rs"]
mod scored;

/// Observational candidate scorer. Replay is a terminated SILK payload without
/// TOC, encoded from the same parameters/pulses as the shared prefix. Config
/// specifies internal rate, channels and samples/channel. Clone incoming decode
/// and resampler state per call; never advance live synthesis state here.
/// Scoring errors abort the transaction and are propagated unchanged.
#[cfg_attr(not(test), allow(dead_code))] // Opt-in integration is wired separately.
pub(super) trait PrefixCandidateScorer {
    fn score(&mut self, config: EncoderConfig, replay: &[u8]) -> Result<i128>;
    #[cfg(test)]
    fn proposal_score(&mut self, _: EncoderConfig, _: &[i16], _: &[i16]) -> Result<Option<i128>> {
        Ok(None)
    }
    #[cfg(test)]
    fn diagnostic_fork(&self) -> Option<Box<dyn PrefixCandidateScorer>> {
        None
    }
}

struct PrefixScoring<'a> {
    scorer: &'a mut dyn PrefixCandidateScorer,
    winner: i128,
}

#[derive(Clone, Copy, PartialEq)]
enum SpectralPolicy {
    Standard,
    CompareCoarse,
}

#[derive(Clone)]
pub(super) struct SilkEncoder {
    pub(super) short_rate_control: bool,
    spectral_policy: SpectralPolicy,
    config: EncoderConfig,
    history: [[i32; 16]; 2],
    gain: [i32; 2],
    previous_index: [i32; 2],
    pcm_history: [Vec<i16>; 2],
    #[cfg(test)]
    transmitted_voiced: usize,
    #[cfg(test)]
    quantizer_cache_disabled: bool,
    #[cfg(test)]
    pub(super) bounded_prefix_search: bool,
    /// Actual quantizations and exact-key hits, including every ranked mask.
    #[cfg(test)]
    quantizer_cache_counts: (usize, usize),
}

type QuantizedFrame = (Vec<i16>, [i32; 16], Vec<i16>);
type SharedQuantizedFrame = Rc<QuantizedFrame>;

// One frame's committed input/state is fixed. Intern exact channel/LPC
// identities once per search, rather than copying LPC vectors on every trial.
type PitchIdentity = (usize, usize, Vec<usize>);
type QuantizerModelIdentity = (usize, Vec<i16>, Option<PitchIdentity>);

type TrialKey = (usize, i32, u32, bool);
struct TrialEntry {
    key: TrialKey,
    frame: SharedQuantizedFrame,
    distortion: i128,
}

#[cfg(test)]
std::thread_local! {
    static TREE_TRIAL_CACHE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Default)]
struct TrialCache {
    models: Vec<Vec<Option<TrialEntry>>>,
    #[cfg(test)]
    tree: BTreeMap<TrialKey, TrialEntry>,
}

impl TrialCache {
    fn slot(key: TrialKey) -> usize {
        assert!(
            (0..64).contains(&key.1),
            "trial gain outside exact cache domain"
        );
        assert!(key.2 < 13, "bounded pressure13 has a separate cache");
        (usize::try_from(key.1).expect("validated gain") * 13 + key.2 as usize) * 2
            + usize::from(key.3)
    }

    fn entry(&self, key: TrialKey) -> Option<&TrialEntry> {
        let slot = Self::slot(key);
        #[cfg(test)]
        if TREE_TRIAL_CACHE.get() {
            return self.tree.get(&key);
        }
        self.models
            .get(key.0)
            .and_then(|model| model.get(slot))
            .and_then(Option::as_ref)
    }

    fn insert(&mut self, key: TrialKey, frame: SharedQuantizedFrame, distortion: i128) {
        let slot = Self::slot(key);
        let entry = TrialEntry {
            key,
            frame,
            distortion,
        };
        #[cfg(test)]
        if TREE_TRIAL_CACHE.get() {
            self.tree.insert(key, entry);
            return;
        }
        if self.models.len() <= key.0 {
            self.models.resize_with(key.0 + 1, Vec::new);
        }
        let model = &mut self.models[key.0];
        if model.is_empty() {
            model.resize_with(64 * 13 * 2, || None);
        }
        model[slot] = Some(entry);
    }

    fn iter(&self) -> impl Iterator<Item = (&TrialKey, &SharedQuantizedFrame)> {
        let indexed = self
            .models
            .iter()
            .flatten()
            .filter_map(Option::as_ref)
            .map(|entry| (&entry.key, &entry.frame));
        #[cfg(test)]
        let indexed = indexed.chain(self.tree.values().map(|entry| (&entry.key, &entry.frame)));
        indexed
    }
}

#[derive(Default)]
struct QuantizerCache {
    models: Vec<QuantizerModelIdentity>,
    // Frame-local: all model masks start from identical PCM, predictive
    // history, gain and configuration. Only the per-channel analysis-rate
    // choice differs; do not redo the full NLSF search for the opposing mask.
    analysis: BTreeMap<(usize, bool), Model>,
    // Bounded zero-pulse pressure=13 is a distinct quantizer domain.
    bounded_frames: BTreeMap<(usize, i32, u32, bool), SharedQuantizedFrame>,
    frames: TrialCache,
}

impl QuantizerCache {
    fn model_id(&mut self, channel: usize, lpc: &[i16], pitch: Option<&Pitch>) -> usize {
        let pitch = pitch.map(|pitch| (pitch.lag, pitch.periodicity, pitch.filters.clone()));
        if let Some(index) =
            self.models
                .iter()
                .position(|(candidate, coefficients, previous_pitch)| {
                    *candidate == channel && coefficients == lpc && *previous_pitch == pitch
                })
        {
            return index;
        }
        let index = self.models.len();
        self.models.push((channel, lpc.to_vec(), pitch));
        index
    }
}

struct PacketCandidate {
    distortion: i128,
    payload: Vec<u8>,
    prefix: Option<RangeEncoder>,
    indices: Vec<i32>,
    gains: Vec<i32>,
    quantized: Vec<SharedQuantizedFrame>,
}

#[derive(Clone, Copy)]
struct PairSearch<'a> {
    inputs: &'a [Vec<i32>],
    models: &'a [Model],
    cache: &'a QuantizerCache,
    model_ids: &'a [usize],
    incoming: &'a RangeEncoder,
    budget: usize,
    rate: usize,
    bandwidth: Bandwidth,
}

// SILK syntax emits only inverse-CDF symbols. Record their validated binary
// intervals once; replay uses the existing arithmetic encoder, not a second
// implementation. Tables/pulse shells are independent of the incoming range.
trait SilkEntropy {
    #[cfg(test)]
    fn semantic_parameters(&mut self, _: &FrameParameters<'_>) {}
    #[cfg(test)]
    fn semantic_pulses(&mut self, _: &[i16]) {}
    fn ec_enc_icdf(
        &mut self,
        symbol: usize,
        table: &[u8],
        bits: u32,
    ) -> std::result::Result<(), crate::range::RangeEncoderError>;
}

impl SilkEntropy for RangeEncoder {
    fn ec_enc_icdf(
        &mut self,
        symbol: usize,
        table: &[u8],
        bits: u32,
    ) -> std::result::Result<(), crate::range::RangeEncoderError> {
        Self::ec_enc_icdf(self, symbol, table, bits)
    }
}

#[cfg(test)]
std::thread_local! {
    pub(super) static TYPED_SCORE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SEMANTIC_CHANNELS: std::cell::RefCell<Vec<SemanticFixture>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
fn record_semantic(value: SemanticFixture) {
    if TYPED_SCORE.get() {
        SEMANTIC_CHANNELS.with_borrow_mut(|channels| {
            channels.push(value);
            if channels.len() > 2 {
                channels.remove(0);
            }
        });
    }
}
#[cfg(test)]
pub(super) fn take_semantic_channels() -> Vec<SemanticFixture> {
    SEMANTIC_CHANNELS.with_borrow_mut(std::mem::take)
}

#[derive(Default)]
struct SilkProgram(Vec<(u32, u32, u32)>, #[cfg(test)] Option<SemanticFixture>);

impl SilkEntropy for SilkProgram {
    #[cfg(test)]
    fn semantic_parameters(&mut self, p: &FrameParameters<'_>) {
        if p.pitch.is_none() && p.bandwidth == Bandwidth::Wideband {
            self.1 = Some((
                p.model.stage1,
                p.model.residual.clone(),
                p.gain.max(p.previous_gain - 16),
                Vec::new(),
            ));
        }
    }
    #[cfg(test)]
    fn semantic_pulses(&mut self, pulses: &[i16]) {
        if let Some(value) = &mut self.1 {
            value.3 = pulses.to_vec();
        }
    }
    fn ec_enc_icdf(
        &mut self,
        symbol: usize,
        table: &[u8],
        bits: u32,
    ) -> std::result::Result<(), crate::range::RangeEncoderError> {
        // Validate using the canonical API once at program construction, then
        // retain precisely the interval that API passes to ec_encode_bin.
        RangeEncoder::new(16)?.ec_enc_icdf(symbol, table, bits)?;
        let total = 1_u32 << bits;
        let previous = if symbol == 0 {
            total
        } else {
            u32::from(table[symbol - 1])
        };
        self.0
            .push((total - previous, total - u32::from(table[symbol]), bits));
        Ok(())
    }
}

impl SilkProgram {
    fn replay(&self, coder: &mut RangeEncoder) -> Result<()> {
        #[cfg(test)]
        if let Some(value) = &self.1 {
            record_semantic(value.clone());
        }
        for &(low, high, bits) in &self.0 {
            coder.ec_encode_bin(low, high, bits).map_err(entropy)?;
        }
        Ok(())
    }

    // Whole-bit tell is monotone as symbols are appended. Once a pair exceeds
    // the actual incoming budget, no remaining symbol can make it a fit. Keep
    // the same candidate frontier/order but avoid finishing impossible replays.
    fn replay_fitting(&self, coder: &mut RangeEncoder, initial: u64, budget: usize) -> bool {
        for symbols in self.0.chunks(8) {
            for &(low, high, bits) in symbols {
                if coder.ec_encode_bin(low, high, bits).is_err() {
                    return false;
                }
            }
            if coder.tell().saturating_sub(initial) as usize > budget {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
std::thread_local! {
    static REFERENCE_PAIRS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Each sorted row is monotone in (distortion,left,right). Merging its head
// produces the same total order as the original grid frontier, including ties,
// with one heap entry per row and no quadratic visited set.
#[cfg(test)]
type ReferenceFrontier = (BTreeSet<(i128, usize, usize)>, BTreeSet<(usize, usize)>);

struct PairQueue {
    heap: std::collections::BinaryHeap<std::cmp::Reverse<(i128, usize, usize)>>,
    #[cfg(test)]
    reference: Option<ReferenceFrontier>,
}

impl PairQueue {
    fn new(candidates: &[Vec<PairCandidate<'_>>]) -> Self {
        #[cfg(test)]
        if REFERENCE_PAIRS.get() {
            return Self {
                heap: std::collections::BinaryHeap::new(),
                reference: Some((
                    BTreeSet::from([(candidates[0][0].0 + candidates[1][0].0, 0, 0)]),
                    BTreeSet::from([(0, 0)]),
                )),
            };
        }
        Self {
            heap: candidates[0]
                .iter()
                .enumerate()
                .map(|(left, frame)| std::cmp::Reverse((frame.0 + candidates[1][0].0, left, 0)))
                .collect(),
            #[cfg(test)]
            reference: None,
        }
    }

    fn pop(&mut self, candidates: &[Vec<PairCandidate<'_>>]) -> Option<(i128, usize, usize)> {
        #[cfg(test)]
        if let Some((pending, visited)) = &mut self.reference {
            let entry = pending.pop_first()?;
            for (left, right) in [(entry.1 + 1, entry.2), (entry.1, entry.2 + 1)] {
                if left < candidates[0].len()
                    && right < candidates[1].len()
                    && visited.insert((left, right))
                {
                    pending.insert((candidates[0][left].0 + candidates[1][right].0, left, right));
                }
            }
            return Some(entry);
        }
        let std::cmp::Reverse(entry) = self.heap.pop()?;
        let next = entry.2 + 1;
        if next < candidates[1].len() {
            self.heap.push(std::cmp::Reverse((
                candidates[0][entry.1].0 + candidates[1][next].0,
                entry.1,
                next,
            )));
        }
        Some(entry)
    }
}

fn pair_programs(
    context: PairSearch<'_>,
    candidates: &[Vec<PairCandidate<'_>>],
    previous_indices: [i32; 2],
    long: bool,
    enabled: bool,
) -> Vec<Vec<Option<SilkProgram>>> {
    if enabled {
        candidates
            .iter()
            .enumerate()
            .map(|(channel, frames)| {
                frames
                    .iter()
                    .map(|&(_, key, frame)| {
                        let mut program = SilkProgram::default();
                        let result = encode_parameters(
                            &mut program,
                            FrameParameters {
                                model: &context.models[channel],
                                gain: key.1,
                                previous_gain: previous_indices[channel],
                                long,
                                bandwidth: context.bandwidth,
                                pitch: None,
                                rate: context.rate,
                            },
                        )
                        .and_then(|()| encode_pulses(&mut program, &frame.0, false));
                        result.map(|()| program).ok()
                    })
                    .collect()
            })
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
fn report_pair_cost(
    tested: usize,
    program_us: u128,
    started: std::time::Instant,
    candidates: &[Vec<PairCandidate<'_>>],
    best: Option<&PacketCandidate>,
) {
    eprintln!(
        "SILK_PAIR_COMPLETE tested={tested} program_us={program_us} total_us={} sizes={:?} best={:?}",
        started.elapsed().as_micros(),
        candidates.iter().map(Vec::len).collect::<Vec<_>>(),
        best.as_ref().map(|frame| frame.distortion)
    );
}

type PairCandidate<'a> = (i128, (usize, i32, u32, bool), &'a SharedQuantizedFrame);

fn pair_candidates(context: PairSearch<'_>, optimized: bool) -> Vec<Vec<PairCandidate<'_>>> {
    (0..2)
        .map(|channel| {
            let mut frames: Vec<_> = context
                .cache
                .frames
                .iter()
                .filter(|(key, _)| key.0 == context.model_ids[channel])
                .map(|(key, frame)| {
                    let distortion = context.inputs[channel]
                        .iter()
                        .zip(&frame.2)
                        .map(|(&target, &sample)| (i128::from(target) - i128::from(sample)).pow(2))
                        .sum::<i128>();
                    (distortion, *key, frame)
                })
                .collect();
            frames.sort_by_key(|&(distortion, key, _)| (distortion, key));
            if optimized {
                // Keep the earliest sorted key. Same gain and complete
                // quantized result imply identical transmitted syntax,
                // distortion and future predictor state; pressure/shaping
                // themselves are not transmitted.
                let mut seen = BTreeSet::new();
                frames.retain(|&(_, key, frame)| seen.insert((key.1, frame.clone())));
            }
            frames
        })
        .collect()
}

#[derive(Clone)]
struct Model {
    stage1: u8,
    residual: Vec<i8>,
    lpc: Vec<i16>,
}

impl SilkEncoder {
    pub(super) const fn set_compact_prefix(&mut self, compact: bool) {
        self.spectral_policy = if compact {
            SpectralPolicy::CompareCoarse
        } else {
            SpectralPolicy::Standard
        };
    }

    pub(super) const fn compact_prefix(&self) -> bool {
        matches!(self.spectral_policy, SpectralPolicy::CompareCoarse)
    }

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
            short_rate_control: false,
            spectral_policy: SpectralPolicy::Standard,
            config,
            history: [[0; 16]; 2],
            gain: [65536; 2],
            previous_index: [10; 2],
            pcm_history: [Vec::new(), Vec::new()],
            #[cfg(test)]
            transmitted_voiced: 0,
            #[cfg(test)]
            quantizer_cache_disabled: false,
            #[cfg(test)]
            bounded_prefix_search: true,
            #[cfg(test)]
            quantizer_cache_counts: (0, 0),
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
        #[cfg(test)]
        cold::capture_prefix(self, pcm, encoder);
        self.encode_internal(pcm, Some(encoder), true).map(|_| ())
    }

    // No additive entropy estimate is used. Distortion alone provides an exact
    // lower bound; every improving pair is checked against the incoming coder.
    // Equal-distortion pairs never replace the historical search winner.
    #[allow(clippy::too_many_lines)] // Exact frontier, entropy feasibility and atomic winner commit.
    fn search_pairs(&self, context: PairSearch<'_>, best: &mut Option<PacketCandidate>) {
        let optimized = true;
        #[cfg(test)]
        let optimized = optimized && !self.quantizer_cache_disabled;
        let candidates = pair_candidates(context, optimized);
        if candidates.iter().any(Vec::is_empty) {
            return;
        }
        let mut header = context.incoming.clone();
        let header_valid = encode_header(&mut header, 2).is_ok();
        // Cache failures too: the incoming state and channel-zero syntax are
        // fixed, independent of whichever channel-one candidate follows.
        let mut prefixes: BTreeMap<usize, Option<RangeEncoder>> = BTreeMap::new();
        let programs_enabled = true;
        #[cfg(test)]
        let programs_enabled = programs_enabled && !REFERENCE_PAIRS.get();
        #[cfg(test)]
        let pair_started = std::time::Instant::now();
        let programs = pair_programs(
            context,
            &candidates,
            self.previous_index,
            self.config.frame_size == context.rate / 50,
            programs_enabled,
        );
        #[cfg(test)]
        let program_us = pair_started.elapsed().as_micros();
        let mut pending = PairQueue::new(&candidates);
        #[cfg(test)]
        let mut tested = 0_usize;
        while let Some((error, left, right)) = pending.pop(&candidates) {
            if best
                .as_ref()
                .is_some_and(|winner| error >= winner.distortion)
            {
                break;
            }
            let pair = [candidates[0][left], candidates[1][right]];
            let encode_channel = |coder: &mut RangeEncoder, channel: usize| -> Result<()> {
                if programs_enabled {
                    return programs[channel][if channel == 0 { left } else { right }]
                        .as_ref()
                        .ok_or_else(|| Error::Unsupported("invalid SILK entropy program".into()))?
                        .replay(coder);
                }
                let (_, key, frame) = pair[channel];
                encode_parameters(
                    coder,
                    FrameParameters {
                        model: &context.models[channel],
                        gain: key.1,
                        previous_gain: self.previous_index[channel],
                        long: self.config.frame_size == context.rate / 50,
                        bandwidth: context.bandwidth,
                        pitch: None,
                        rate: context.rate,
                    },
                )?;
                encode_pulses(coder, &frame.0, false)
            };
            let mut coder = if optimized {
                prefixes
                    .entry(left)
                    .or_insert_with(|| {
                        let mut prefix = header.clone();
                        (header_valid && encode_channel(&mut prefix, 0).is_ok()).then_some(prefix)
                    })
                    .clone()
            } else {
                let mut prefix = context.incoming.clone();
                (encode_header(&mut prefix, 2).is_ok() && encode_channel(&mut prefix, 0).is_ok())
                    .then_some(prefix)
            };
            let fits = coder.as_mut().is_some_and(|state| {
                let replay_fits = if programs_enabled {
                    programs[1][right].as_ref().is_some_and(|program| {
                        program.replay_fitting(state, context.incoming.tell(), context.budget)
                    })
                } else {
                    encode_channel(state, 1).is_ok()
                };
                replay_fits
                    && state.tell().saturating_sub(context.incoming.tell()) as usize
                        <= context.budget
            });
            #[cfg(test)]
            {
                tested += 1;
            }
            if fits {
                let coder = coder.expect("fitting pair has entropy state");
                *best = Some(PacketCandidate {
                    distortion: error,
                    payload: Vec::new(),
                    prefix: Some(coder),
                    indices: pair.iter().map(|entry| entry.1.1).collect(),
                    gains: pair
                        .iter()
                        .map(|entry| SilkDecoder::dequantize_gain(entry.1.1))
                        .collect(),
                    quantized: pair.iter().map(|entry| entry.2.clone()).collect(),
                });
                break;
            }
        }
        #[cfg(test)]
        report_pair_cost(tested, program_us, pair_started, &candidates, best.as_ref());
    }

    fn commit_quantized(
        &mut self,
        quantized: &[SharedQuantizedFrame],
        gains: &[i32],
        indices: &[i32],
        rate: usize,
    ) {
        for (channel, frame) in quantized.iter().enumerate() {
            let (_, history, pcm) = frame.as_ref();
            self.history[channel] = *history;
            self.gain[channel] = gains[channel];
            self.previous_index[channel] = indices[channel];
            self.pcm_history[channel].extend_from_slice(pcm);
            let excess = self.pcm_history[channel].len().saturating_sub(rate / 50);
            self.pcm_history[channel].drain(..excess);
        }
    }

    // Bounded native-objective refinement. Raw search supplies a legal seed;
    // every coordinate proposal is evaluated without a raw-error bound. This
    // does NOT claim global optimality over the Cartesian product of channels.
    fn score_stereo_coordinates(
        &self,
        context: PairSearch<'_>,
        best: &mut Option<PacketCandidate>,
        scoring: &mut PrefixScoring<'_>,
    ) -> Result<()> {
        let Some(mut winner) = best.take() else {
            return Ok(());
        };
        let evaluate = |indices: &[i32],
                        frames: &[SharedQuantizedFrame],
                        scorer: &mut dyn PrefixCandidateScorer|
         -> Result<Option<(i128, RangeEncoder)>> {
            let encode = |coder: &mut RangeEncoder| -> Result<()> {
                encode_header(coder, 2)?;
                for channel in 0..2 {
                    encode_parameters(
                        coder,
                        FrameParameters {
                            model: &context.models[channel],
                            gain: indices[channel],
                            previous_gain: self.previous_index[channel],
                            long: self.config.frame_size == context.rate / 50,
                            bandwidth: context.bandwidth,
                            pitch: None,
                            rate: context.rate,
                        },
                    )?;
                    encode_pulses(coder, &frames[channel].0, false)?;
                }
                Ok(())
            };
            let mut coder = context.incoming.clone();
            if encode(&mut coder).is_err()
                || coder.tell().saturating_sub(context.incoming.tell()) as usize > context.budget
            {
                return Ok(None);
            }
            let replay = replay::encode(encode)?;
            Ok(Some((scorer.score(self.config, &replay)?, coder)))
        };
        let (error, coder) = evaluate(&winner.indices, &winner.quantized, scoring.scorer)?
            .expect("raw baseline satisfies identical shared budget");
        winner.distortion = error;
        winner.prefix = Some(coder);
        // Cache order is stable (model, gain, pressure, shaping). Two passes
        // through both channels keep the opposing component fixed per proposal.
        for _ in 0..2 {
            for channel in 0..2 {
                for (key, frame) in context.cache.frames.iter() {
                    if key.0 != context.model_ids[channel] {
                        continue;
                    }
                    let mut indices = winner.indices.clone();
                    let mut frames = winner.quantized.clone();
                    indices[channel] = key.1;
                    frames[channel] = frame.clone();
                    if let Some((error, coder)) = evaluate(&indices, &frames, scoring.scorer)?
                        && error < winner.distortion
                    {
                        winner.distortion = error;
                        winner.prefix = Some(coder);
                        winner.indices = indices;
                        winner.gains[channel] = SilkDecoder::dequantize_gain(key.1);
                        winner.quantized = frames;
                    }
                }
            }
        }
        #[cfg(test)]
        if std::env::var_os("MOOSICBOX_OPUS_BOUNDED_TRACE").is_some() {
            let keys: Vec<_> = (0..2)
                .map(|channel| {
                    context
                        .cache
                        .frames
                        .iter()
                        .find(|(key, frame)| {
                            key.0 == context.model_ids[channel]
                                && key.1 == winner.indices[channel]
                                && ***frame == *winner.quantized[channel]
                        })
                        .map(|(key, _)| *key)
                })
                .collect();
            eprintln!(
                "SILK_EXHAUSTIVE_WIN score={} keys={keys:?}",
                winner.distortion
            );
        }
        scoring.winner = winner.distortion;
        *best = Some(winner);
        Ok(())
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
                unshaped: false,
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
        if shared.is_none() {
            if self.config.bitrate / super::channel_count(self.config.channels) as u32 > 32_000 {
                let mut legacy = self.clone();
                if let Ok(packet) = legacy.encode_search_policy(
                    pcm,
                    None,
                    false,
                    0,
                    &mut QuantizerCache::default(),
                    None,
                    false,
                ) && legacy.standalone_reconstruction_suitable(pcm)
                {
                    *self = legacy;
                    return Ok(packet);
                }
            }
            return self.encode_standalone_ranked(pcm);
        }
        if rank_prefix
            && self.config.channels == crate::Channels::Stereo
            && self.config.bitrate <= 32_000
        {
            return self
                .encode_model_ranked(pcm, shared.expect("ranked prefix requires entropy state"));
        }
        self.encode_search(
            pcm,
            shared,
            rank_prefix,
            0,
            &mut QuantizerCache::default(),
            None,
        )
    }

    // Quantization synthesizes the transmitted excitation using the committed
    // decoder history. Inspect only this candidate frame, not older output.
    // A candidate which clips cannot be retained merely for compatibility.
    fn standalone_reconstruction_suitable(&self, pcm: &[f32]) -> bool {
        let channels = super::channel_count(self.config.channels);
        let length = pcm.len() / channels;
        let mut signal = [0_i128; 2];
        let mut error = [0_i128; 2];
        for (i, frame) in pcm.chunks_exact(channels).enumerate() {
            let component = |channel: usize| {
                let history = &self.pcm_history[channel];
                i32::from(history[history.len() - length + i])
            };
            let mid = component(0);
            let side = if channels == 2 { component(1) } else { 0 };
            for channel in 0..channels {
                let reconstructed = if channels == 1 {
                    mid
                } else if channel == 0 {
                    mid + side
                } else {
                    mid - side
                };
                if reconstructed <= i32::from(i16::MIN) || reconstructed >= i32::from(i16::MAX) {
                    return false;
                }
                let target = (f64::from(frame[channel]) * 32768.0).round() as i128;
                signal[channel] += target * target;
                error[channel] += (target - i128::from(reconstructed)).pow(2);
            }
        }
        // Reuse the public 3 dB waveform gate, not a bitrate-specific tolerance.
        // Zero input has no defined SNR; clipping remains forbidden there.
        // 10^(3/10), represented to nine decimal places. Keep the PCM score
        // integer like the quantizer; no platform log/libm decision in coding.
        (0..channels)
            .all(|c| signal[c] == 0 || signal[c] * 1_000_000_000 > error[c] * 1_995_262_315)
    }

    // Rate-only open-loop model choice can make all budget fits ring, especially
    // after quiet input. Compare transmitted reconstructions, including the
    // historical spectral/pitch policy, before committing predictive state.
    fn encode_standalone_ranked(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        let channels = super::channel_count(self.config.channels);
        let mut best: Option<(i128, Self, Vec<u8>)> = None;
        let mut failure = None;
        let masks = if channels == 1 {
            vec![0, 1, 4, 5, 16, 17, 20, 21]
        } else {
            vec![0, 1, 2, 3, 16, 17, 18, 19]
        };
        let mut cache = QuantizerCache::default();
        for mask in masks {
            if mask == 16 && best.is_some() {
                break;
            }
            let mut candidate = self.clone();
            // Exact pitch and LPC identity are included in the frame-local
            // cache; entropy/model signaling remains separately evaluated.
            let result = candidate.encode_search(pcm, None, false, mask, &mut cache, None);
            let packet = match result {
                Ok(packet) => packet,
                Err(error) => {
                    failure = Some(error);
                    continue;
                }
            };
            let mut distortion = 0_i128;
            for (i, frame) in pcm.chunks_exact(channels).enumerate() {
                let left = (f64::from(frame[0]) * 32768.0).round() as i32;
                let right = if channels == 2 {
                    (f64::from(frame[1]) * 32768.0).round() as i32
                } else {
                    left
                };
                let targets = if channels == 1 {
                    [left, 0]
                } else {
                    [left.midpoint(right), (left - right) / 2]
                };
                for (channel, target) in targets.into_iter().take(channels).enumerate() {
                    let history = &candidate.pcm_history[channel];
                    distortion += (i128::from(target)
                        - i128::from(history[history.len() - self.config.frame_size + i]))
                    .pow(2);
                }
            }
            if best.as_ref().is_none_or(|previous| distortion < previous.0) {
                best = Some((distortion, candidate, packet));
            }
        }
        #[cfg(test)]
        if std::env::var_os("MOOSICBOX_OPUS_SILK_SEARCH_PROFILE").is_some() {
            eprintln!(
                "SILK_CONTEXTS masks={} unique={}",
                if channels == 1 { 4 } else { 8 },
                cache.models.len()
            );
        }
        if let Some((_, candidate, packet)) = best {
            *self = candidate;
            Ok(packet)
        } else {
            Err(failure.expect("all standalone spectral candidates failed"))
        }
    }

    /// Fits 16 kHz 10/20 ms prefixes with an observational synthesis score.
    /// Mono scores every fitting trial. Stereo preserves each model's raw-search
    /// baseline, then performs two native-score coordinate passes through cached
    /// gain/pressure/shaping candidates; it is not globally pair-optimal.
    /// Errors leave encoder and incoming entropy state unchanged.
    #[cfg_attr(not(test), allow(dead_code))] // Opt-in owner integration follows.
    pub(super) fn encode_prefix_scored(
        &mut self,
        pcm: &[f32],
        shared: &mut RangeEncoder,
        scorer: &mut dyn PrefixCandidateScorer,
    ) -> Result<()> {
        if self.config.sample_rate != SampleRate::Hz16000
            || ![160, 320].contains(&self.config.frame_size)
            || pcm.len() != self.config.frame_size * super::channel_count(self.config.channels)
        {
            return Err(Error::Unsupported(
                "scored SILK prefix requires 16 kHz 10/20 ms".into(),
            ));
        }
        let mut cache = QuantizerCache::default();
        let mut best = None;
        let masks: &[u8] = if self.config.channels == crate::Channels::Mono {
            &[0, 1, 4, 5]
        } else if self.compact_prefix() {
            &[16, 19, 0, 3]
        } else if self.short_rate_control {
            &[0]
        } else if self.config.frame_size == 320 {
            &[0, 3]
        } else {
            &[0, 1, 2, 3]
        };
        for &mask in masks {
            let mut candidate = self.clone();
            let mut range = shared.clone();
            let mut scoring = PrefixScoring {
                scorer,
                winner: i128::MAX,
            };
            candidate.encode_search(
                pcm,
                Some(&mut range),
                true,
                mask,
                &mut cache,
                Some(&mut scoring),
            )?;
            if best
                .as_ref()
                .is_none_or(|(error, _, _)| scoring.winner < *error)
            {
                best = Some((scoring.winner, candidate, range));
            }
        }
        let (_, candidate, range) = best.expect("nonempty scored model set");
        *self = candidate;
        *shared = range;
        Ok(())
    }

    // Every spectral combination starts from the identical committed predictor
    // and real incoming range state. Fit and rank the transmitted reconstruction,
    // not open-loop LPC prediction error. The baseline model is always included.
    fn encode_model_ranked(&mut self, pcm: &[f32], shared: &mut RangeEncoder) -> Result<Vec<u8>> {
        let mut best: Option<(i128, Self, RangeEncoder)> = None;
        let mut failure = None;
        // Every mask starts from self's identical input, old gains and histories.
        // Hybrid pitch is None; exact channel/LPC identity distinguishes the
        // only quantizer input that changes between spectral masks.
        let mut cache = QuantizerCache::default();
        #[cfg(test)]
        let mut counts = self.quantizer_cache_counts;
        for mask in 0..4 {
            let mut candidate = self.clone();
            let mut range = shared.clone();
            let result =
                candidate.encode_search(pcm, Some(&mut range), true, mask, &mut cache, None);
            #[cfg(test)]
            {
                counts.0 += candidate.quantizer_cache_counts.0 - self.quantizer_cache_counts.0;
                counts.1 += candidate.quantizer_cache_counts.1 - self.quantizer_cache_counts.1;
            }
            if let Err(error) = result {
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
            #[cfg(test)]
            {
                self.quantizer_cache_counts = counts;
            }
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
        shared: Option<&mut RangeEncoder>,
        rank_prefix: bool,
        model_mask: u8,
        quantizer_cache: &mut QuantizerCache,
        scoring: Option<&mut PrefixScoring<'_>>,
    ) -> Result<Vec<u8>> {
        self.encode_search_policy(
            pcm,
            shared,
            rank_prefix,
            model_mask,
            quantizer_cache,
            scoring,
            true,
        )
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)] // Ordered entropy search with explicit standalone coverage.
    fn encode_search_policy(
        &mut self,
        pcm: &[f32],
        mut shared: Option<&mut RangeEncoder>,
        rank_prefix: bool,
        model_mask: u8,
        quantizer_cache: &mut QuantizerCache,
        mut scoring: Option<&mut PrefixScoring<'_>>,
        expanded_standalone: bool,
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
        #[cfg(test)]
        let model_started = std::time::Instant::now();
        let mut models: Vec<_> = inputs
            .iter()
            .enumerate()
            .map(|(channel, input)| {
                let choice = (channel, model_mask & (1 << channel) != 0);
                let reuse_analysis = true;
                #[cfg(test)]
                let reuse_analysis = reuse_analysis && !self.quantizer_cache_disabled;
                if reuse_analysis && let Some(model) = quantizer_cache.analysis.get(&choice) {
                    return Ok(model.clone());
                }
                let model = if input.iter().all(|&sample| sample == 0) {
                    self.silence_model(input, channel, bandwidth, rate)?
                } else {
                    let mut analysis: Vec<i32> = self.history[channel]
                        .iter()
                        .map(|&x| {
                            ((i64::from(x) * i64::from(self.gain[channel])) >> 30)
                                .clamp(-32768, 32767) as i32
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
                    )?
                };
                if reuse_analysis {
                    quantizer_cache.analysis.insert(choice, model.clone());
                }
                Ok(model)
            })
            .collect::<Result<_>>()?;
        #[cfg(test)]
        let model_ns = model_started.elapsed().as_nanos();
        // If all analysis models are infeasible, probe a finite coarse-NLSF
        // fallback. Zero residuals are legal codebook symbols, not zeroed PCM
        // or dropped side-channel excitation. Reconstruct the transmitted LPC.
        if model_mask & 16 != 0 {
            for model in &mut models {
                model.residual.fill(0);
                let lsf = SilkDecoder::reconstruct_lsf(model.stage1, &model.residual, bandwidth)?;
                model.lpc = SilkDecoder::limit_lpc_coefficients(&lsf, bandwidth)?;
            }
        }
        let pitches: Vec<_> = inputs
            .iter()
            .enumerate()
            .map(|(c, input)| {
                // Keep hybrid and stereo on the established unvoiced path until
                // their predictor/delay adaptation is qualified independently.
                if channels == 1 && shared.is_none() && model_mask & 4 == 0 {
                    prediction::analyze(input, &self.pcm_history[c], &models[c].lpc, rate)
                } else {
                    None
                }
            })
            .collect();
        let bytes =
            (self.config.bitrate as usize * self.config.frame_size / (rate * 8)).saturating_sub(1);
        #[cfg(not(test))]
        let bounded_enabled = true;
        #[cfg(test)]
        let bounded_enabled = self.bounded_prefix_search;
        let feasibility_fallback =
            bounded_enabled && channels == 2 && rank_prefix && scoring.is_some();
        if bounded_enabled
            && channels == 2
            && rank_prefix
            && let Some(scoring) = scoring.as_deref_mut()
        {
            #[cfg(test)]
            let bounded_started = std::time::Instant::now();
            let result = self.bounded_stereo_search(
                &inputs,
                &models,
                shared
                    .as_deref()
                    .expect("bounded prefix requires shared range"),
                scoring.scorer,
                quantizer_cache,
            )?;
            #[cfg(test)]
            if std::env::var_os("MOOSICBOX_OPUS_RD_PHASE_PROFILE").is_some() {
                eprintln!(
                    "SILK_MODEL_PHASE mask={model_mask} model_ns={model_ns} bounded_ns={}",
                    bounded_started.elapsed().as_nanos()
                );
            }
            if let Some(candidate) = result {
                scoring.winner = candidate.distortion;
                *shared.expect("bounded prefix requires shared range") = candidate
                    .prefix
                    .expect("bounded prefix carries range state");
                self.commit_quantized(
                    &candidate.quantized,
                    &candidate.gains,
                    &candidate.indices,
                    rate,
                );
                return Ok(Vec::new());
            }
        }
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
        let independent_pressure = (rank_prefix && channels == 2)
            || (shared.is_none()
                && (per_channel_bitrate <= 24_000
                    || (self.config.frame_size == rate / 100 && per_channel_bitrate <= 32_000)));
        let baseline_search_trials = baseline_trials + if independent_pressure { 122 } else { 0 };
        // The sparse search is anchored to the first fit at gain 63. On a
        // persistent predictor, useful fits can lie below its eight coarse
        // refinement gains, or between them at a different pulse pressure.
        // Cover every legal gain at each existing pressure for low-rate
        // shared prefixes, retaining every historical candidate. Do not
        // change standalone or accepted high-rate prefix search policy.
        // A standalone packet also needs the complete gain/pressure lattice:
        // at medium/wideband the gain-63 first fit can be a clipped zero-pulse
        // reconstruction, not a useful anchor for sparse local refinement.
        // Standalone high-rate frames also need independent gain/pressure
        // coverage: a sharp model can otherwise fit only at gain 63 and ring
        // into clipping. Keep the shared-prefix policy unchanged.
        let complete_prefix_search = (shared.is_none() && expanded_standalone)
            || ((shared.is_none() || rank_prefix) && per_channel_bitrate <= 32_000);
        let trials = baseline_search_trials + if complete_prefix_search { 64 * 13 } else { 0 };
        let common_trials = trials * gain_offsets.len() as i32;
        // Refine one component while retaining the other's exact winning
        // pulses. A shared pulse pressure cannot allocate bits independently
        // even when both RMS gain offsets are nearly zero.
        // Retain the shaped passes, then append unshaped coordinate passes.
        // Noise shaping can overshoot raw PCM at sparse budgets; rank both
        // using transmitted reconstruction under the same real range budget.
        let coordinate_trials = if (rank_prefix || shared.is_none())
            && channels == 2
            && per_channel_bitrate <= 16_000
        {
            4 * 2 * 17 * 13
        } else {
            0
        };
        let mut coordinate_anchor = [0_i32; 2];
        // The cache is scoped to this frame's fixed input/committed state.
        // Exact LPC identity also allows reuse across the ranked model masks.
        // Keep entropy trials and first-fit updates in their original order.
        let model_ids: Vec<_> = models
            .iter()
            .enumerate()
            .map(|(channel, model)| {
                quantizer_cache.model_id(channel, &model.lpc, pitches[channel].as_ref())
            })
            .collect();
        #[cfg(test)]
        let mut cache_hits = 0_usize;
        #[cfg(test)]
        let started = std::time::Instant::now();
        #[cfg(test)]
        let mut pruned_trials = 0_usize;
        #[cfg(test)]
        let mut entropy_trials = 0_usize;
        #[cfg(test)]
        let mut quantizer_calls = 0_usize;
        #[cfg(test)]
        let mut quantizer_elapsed = std::time::Duration::ZERO;
        let cache_enabled = true;
        #[cfg(test)]
        let cache_enabled = cache_enabled && !self.quantizer_cache_disabled;
        #[cfg(test)]
        let mut fallback_calls = 0_usize;
        let search_trials = if feasibility_fallback {
            (common_trials + coordinate_trials).min(16)
        } else {
            common_trials + coordinate_trials
        };
        for search_trial in 0..search_trials {
            #[cfg(test)]
            if feasibility_fallback {
                fallback_calls += 1;
            }
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
            let (gain_index, rate_pressure) = if feasibility_fallback {
                // Coarse descending coverage of the legal 0..63 gain lattice.
                // Maximum pulse penalty; stop at the first legal fit, not RD.
                (63 - 4 * search_trial, 12)
            } else if coordinate {
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
            let mut channel_distortions = [0_i128; 2];
            let quantized: Vec<_> = (0..channels)
                .map(|c| {
                    if coordinate && c != coordinate_channel {
                        let frame = best
                            .as_ref()
                            .expect("coordinate search requires a fit")
                            .quantized[c]
                            .clone();
                        channel_distortions[c] = inputs[c]
                            .iter()
                            .zip(&frame.2)
                            .map(|(&target, &sample)| {
                                (i128::from(target) - i128::from(sample)).pow(2)
                            })
                            .sum();
                        return frame;
                    }
                    let unshaped = coordinate && trial >= 2 * 2 * 17 * 13;
                    let key = (model_ids[c], indices[c], rate_pressure, unshaped);
                    if cache_enabled && let Some(entry) = quantizer_cache.frames.entry(key) {
                        #[cfg(test)]
                        {
                            cache_hits += 1;
                        }
                        channel_distortions[c] = entry.distortion;
                        return entry.frame.clone();
                    }
                    #[cfg(test)]
                    {
                        quantizer_calls += 1;
                    }
                    #[cfg(test)]
                    let quantizer_started = std::time::Instant::now();
                    let quantized = Rc::new(quantize(QuantizerInput {
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
                        unshaped,
                    }));
                    #[cfg(test)]
                    {
                        quantizer_elapsed += quantizer_started.elapsed();
                    }
                    let distortion = inputs[c]
                        .iter()
                        .zip(&quantized.2)
                        .map(|(&target, &sample)| (i128::from(target) - i128::from(sample)).pow(2))
                        .sum();
                    channel_distortions[c] = distortion;
                    quantizer_cache
                        .frames
                        .insert(key, quantized.clone(), distortion);
                    quantized
                })
                .collect();
            let distortion: i128 = channel_distortions[..channels].iter().sum();
            // First-fit affects later gain proposals and resets for each offset.
            // Preserve all entropy work until it is known, then an equal/worse
            // reconstruction cannot replace the strict-distortion winner.
            if (scoring.is_none() || channels == 2)
                && cache_enabled
                && optimize_distortion
                && first_fit.is_some()
                && best
                    .as_ref()
                    .is_some_and(|winner| distortion >= winner.distortion)
            {
                #[cfg(test)]
                {
                    pruned_trials += 1;
                }
                continue;
            }
            #[cfg(test)]
            {
                entropy_trials += 1;
            }
            let encode = |encoder: &mut RangeEncoder| -> Result<()> {
                encode_header(encoder, channels)?;
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
                #[cfg(test)]
                if std::env::var_os("MOOSICBOX_OPUS_SILK_TRIAL_TRACE").is_some() {
                    cold::trace_trial(
                        (search_trial, rate_pressure, prefix_bits, bytes * 8),
                        &indices,
                        &inputs,
                        &quantized,
                    );
                }
                if prefix_bits as usize > bytes * 8 {
                    last_failure =
                        format!("trial {trial}: prefix {prefix_bits} bits > {}", bytes * 8);
                    continue;
                }
                first_fit.get_or_insert(gain_index);
                let distortion = if let Some(scoring) = scoring
                    .as_deref_mut()
                    .filter(|_| channels == 1 || feasibility_fallback)
                {
                    // Replay independently: never terminate the incoming shared
                    // stream, whose entropy state and prefix budget stay exact.
                    let replay = replay::encode(encode)?;
                    scoring.scorer.score(self.config, &replay)?
                } else {
                    distortion
                };

                if best
                    .as_ref()
                    .is_none_or(|previous| distortion < previous.distortion)
                {
                    #[cfg(test)]
                    if channels == 2 && self.config.bitrate <= 48_000 {
                        let errors: Vec<i128> = inputs
                            .iter()
                            .zip(&quantized)
                            .map(|(input, frame)| {
                                let pcm = &frame.2;
                                input
                                    .iter()
                                    .zip(pcm)
                                    .map(|(&x, &y)| (i128::from(x) - i128::from(y)).pow(2))
                                    .sum()
                            })
                            .collect();
                        let clipping: Vec<usize> = quantized
                            .iter()
                            .map(|frame| {
                                let pcm = &frame.2;
                                pcm.iter()
                                    .filter(|&&x| x == i16::MIN || x == i16::MAX)
                                    .count()
                            })
                            .collect();
                        eprintln!(
                            "SILK_PREFIX_IMPROVEMENT trial={trial} coordinate={coordinate} channel={coordinate_channel} pressure={rate_pressure} offsets={offsets:?} indices={indices:?} bits={prefix_bits} distortion={distortion} errors={errors:?} clipped={clipping:?}"
                        );
                    }
                    if let Some(scoring) = scoring.as_deref_mut() {
                        scoring.winner = distortion;
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
                #[cfg(test)]
                if feasibility_fallback {
                    break;
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
                        .map(|frame| frame.0.iter().map(|q| q.unsigned_abs()).max())
                        .collect::<Vec<_>>(),
                    quantized
                        .iter()
                        .map(|frame| frame
                            .0
                            .iter()
                            .map(|q| u64::from(q.unsigned_abs()))
                            .sum::<u64>())
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
        #[cfg(test)]
        if feasibility_fallback {
            eprintln!(
                "SILK_BOUNDED_FALLBACK calls={fallback_calls} first_fit={} tail_us={}",
                best.is_some(),
                started.elapsed().as_micros()
            );
        }
        if rank_prefix && channels == 2 && per_channel_bitrate <= 24_000 && !feasibility_fallback {
            self.search_pairs(
                PairSearch {
                    inputs: &inputs,
                    models: &models,
                    cache: quantizer_cache,
                    model_ids: &model_ids,
                    incoming: shared.as_deref().expect("ranked prefix has entropy state"),
                    budget: bytes * 8,
                    rate,
                    bandwidth,
                },
                &mut best,
            );
        }
        if channels == 2
            && !feasibility_fallback
            && let Some(scoring) = scoring
        {
            self.score_stereo_coordinates(
                PairSearch {
                    inputs: &inputs,
                    models: &models,
                    cache: quantizer_cache,
                    model_ids: &model_ids,
                    incoming: shared.as_deref().expect("scored prefix has entropy state"),
                    budget: bytes * 8,
                    rate,
                    bandwidth,
                },
                &mut best,
                scoring,
            )?;
        }
        #[cfg(test)]
        if std::env::var_os("MOOSICBOX_OPUS_SILK_SEARCH_PROFILE").is_some() {
            eprintln!(
                "SILK_SEARCH_PROFILE mask={model_mask} quantizations={quantizer_calls} hits={cache_hits} entropy_trials={entropy_trials} pruned={pruned_trials} quantizer_us={} elapsed_us={}",
                quantizer_elapsed.as_micros(),
                started.elapsed().as_micros()
            );
        }
        #[cfg(test)]
        {
            self.quantizer_cache_counts.0 += quantizer_calls;
            self.quantizer_cache_counts.1 += cache_hits;
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
        #[cfg(test)]
        if std::env::var_os("MOOSICBOX_OPUS_SILK_BUDGET_DIAGNOSTIC").is_some() {
            let mut diagnostic = RangeEncoder::new(1024).map_err(entropy)?;
            encode_header(&mut diagnostic, channels)?;
            let mut costs = vec![diagnostic.tell()];
            for (c, model) in models.iter().enumerate() {
                encode_parameters(
                    &mut diagnostic,
                    FrameParameters {
                        model,
                        gain: 63,
                        previous_gain: self.previous_index[c],
                        long: self.config.frame_size == rate / 50,
                        bandwidth,
                        pitch: None,
                        rate,
                    },
                )?;
                costs.push(diagnostic.tell());
                encode_pulses(&mut diagnostic, &vec![0; self.config.frame_size], false)?;
                costs.push(diagnostic.tell());
            }
            eprintln!(
                "BUDGET_ROOT mask={model_mask} payload_bits={} cumulative_header_parameters_pulses={costs:?} measured_bytes={} residuals={:?}",
                bytes * 8,
                diagnostic.tell().div_ceil(8),
                models.iter().map(|m| &m.residual).collect::<Vec<_>>()
            );
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
    // Reverse once per candidate, then score contiguous, fixed-size windows.
    // Keep the initial partial predictor and its Q12 rounding unchanged.
    fn fixed<const N: usize>(input: &[i32], coefficients: &[i16]) -> i128 {
        let taps: [i64; N] = std::array::from_fn(|j| i64::from(coefficients[N - 1 - j]));
        let mut error = 0_i128;
        for i in 0..N.min(input.len()) {
            let dot: i64 = input[..i]
                .iter()
                .zip(&taps[N - i..])
                .map(|(&x, &a)| i64::from(x) * a)
                .sum();
            error += i128::from(i64::from(input[i]) - (dot >> 12)).pow(2);
        }
        for row in input.windows(N + 1) {
            let dot: i64 = (0..N).map(|j| i64::from(row[j]) * taps[j]).sum();
            error += i128::from(i64::from(row[N]) - (dot >> 12)).pow(2);
        }
        error
    }
    match lpc.len() {
        10 => fixed::<10>(input, lpc),
        16 => fixed::<16>(input, lpc),
        _ => unreachable!("SILK LPC order"),
    }
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
    unshaped: bool,
}

const fn shaping_coefficient(bitrate: u32, unshaped: bool) -> i64 {
    if unshaped {
        0
    } else if bitrate < 24000 {
        24576_i64
    } else {
        8192_i64
    }
}

fn quantize(context: QuantizerInput<'_>) -> (Vec<i16>, [i32; 16], Vec<i16>) {
    quantize_levels(context, false)
}

fn quantize_levels(
    context: QuantizerInput<'_>,
    zero_pulses: bool,
) -> (Vec<i16>, [i32; 16], Vec<i16>) {
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
        unshaped,
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
    let shaping = shaping_coefficient(bitrate, unshaped);
    let offset = if pitch.is_some() { 512 } else { 1600 };
    let lambda = (if bitrate < 24000 { 1800_i128 } else { 300_i128 }) << rate_pressure;
    let pulse_cost = pulse::PulseCost {
        lambda,
        shrinkage: (lambda / 2) as i64,
        optimize: optimize_pulse_cost,
    };
    #[cfg(test)]
    let reference_pulses = pulse::REFERENCE.get();
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
        #[cfg(test)]
        let pulse = if reference_pulses {
            pulse_cost.reference(residual)
        } else {
            pulse_cost.select(residual)
        };
        #[cfg(not(test))]
        let pulse = pulse_cost.select(residual);
        let pulse = if zero_pulses { 0 } else { pulse };
        let level = i32::from(pulse) * 16384 - i32::from(pulse.signum()) * 1280 + offset;
        let excitation = (i64::from(level) * sign) as i32;
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

fn encode_header(encoder: &mut RangeEncoder, channels: usize) -> Result<()> {
    for _ in 0..channels {
        encoder.ec_enc_bit_logp(true, 1).map_err(entropy)?;
        encoder.ec_enc_bit_logp(false, 1).map_err(entropy)?;
    }
    if channels == 2 {
        use silk::constants::{
            STEREO_WEIGHT_PDF_STAGE1, STEREO_WEIGHT_PDF_STAGE2, STEREO_WEIGHT_PDF_STAGE3,
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
    Ok(())
}

fn encode_parameters(enc: &mut impl SilkEntropy, parameters: FrameParameters<'_>) -> Result<()> {
    use silk::constants::{
        FRAME_TYPE_PDF_ACTIVE, GAIN_PDF_DELTA, GAIN_PDF_LSB, GAIN_PDF_UNVOICED, GAIN_PDF_VOICED,
    };

    #[cfg(test)]
    enc.semantic_parameters(&parameters);
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

#[allow(clippy::too_many_lines)] // Ordered shell, LSB and sign entropy syntax plus test semantic hook.
fn encode_pulses(enc: &mut impl SilkEntropy, pulses: &[i16], voiced: bool) -> Result<()> {
    #[cfg(test)]
    enc.semantic_pulses(pulses);
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

#[cfg(test)]
#[test]
fn bounded_entropy_replay_matches_complete_budget_decision() {
    let mut program = SilkProgram::default();
    for i in 0..600 {
        program.ec_enc_icdf(i % 4, &[210, 110, 35, 0], 8).unwrap();
    }
    for leading in [0, 3, 19] {
        let mut incoming = RangeEncoder::new(512).unwrap();
        for i in 0..leading {
            incoming.ec_enc_bit_logp(i % 2 == 0, 3).unwrap();
        }
        // Include nonzero head bytes, pending carry chains and unrelated raw
        // tail bits; only forward arithmetic symbols are budget-short-circuited.
        for i in 0..97 {
            incoming
                .ec_enc_icdf((i * 13 + leading) % 4, &[255, 170, 3, 0], 8)
                .unwrap();
        }
        incoming.ec_enc_bits(0b101_0110, 7).unwrap();
        let initial = incoming.tell();
        let mut complete = incoming.clone();
        program.replay(&mut complete).unwrap();
        let used = complete.tell().saturating_sub(initial) as usize;
        for budget in [0, 8, used - 1, used, used + 1] {
            let mut bounded = incoming.clone();
            assert_eq!(
                program.replay_fitting(&mut bounded, initial, budget),
                used <= budget
            );
            if used <= budget {
                assert_eq!(
                    bounded.finish().unwrap(),
                    complete.clone().finish().unwrap()
                );
            }
        }
    }
}

#[cfg(test)]
pub(super) type SemanticFixture = (u8, Vec<i8>, i32, Vec<i16>);

#[cfg(test)]
pub(super) fn component_reuse_fixture(frame_size: usize, scale: f32) -> Vec<Vec<u8>> {
    component_semantic_fixture(frame_size, scale).0
}

#[cfg(test)]
pub(super) fn component_semantic_fixture(
    frame_size: usize,
    scale: f32,
) -> (Vec<Vec<u8>>, Vec<Vec<SemanticFixture>>) {
    let config = EncoderConfig {
        sample_rate: SampleRate::Hz16000,
        channels: crate::Channels::Stereo,
        bitrate: 64000,
        frame_size,
    };
    let state = SilkEncoder::new(config).unwrap();
    let mut programs = Vec::new();
    let mut semantic_channels = Vec::new();
    for channel in 0..2 {
        let mut choices = Vec::new();
        let mut semantics = Vec::new();
        for choice in 0..2 {
            let input: Vec<i32> = (0..frame_size)
                .map(|i| {
                    ((std::f32::consts::TAU * (173 + channel * 113) as f32 * i as f32 / 16000.0)
                        .sin()
                        * scale
                        * 32768.0) as i32
                })
                .collect();
            let model = analyze_rate(&input, Bandwidth::Wideband, 32000).unwrap();
            let index = 35 + choice * 4;
            let quantized = quantize(QuantizerInput {
                input: &input,
                lpc: &model.lpc,
                history: &state.history[channel],
                old_gain: state.gain[channel],
                gain: SilkDecoder::dequantize_gain(index),
                pitch: None,
                previous_pcm: &[],
                rate: 16000,
                bitrate: 32000,
                rate_pressure: choice as u32 * 3,
                optimize_pulse_cost: true,
                unshaped: choice != 0,
            });
            let mut program = SilkProgram::default();
            encode_parameters(
                &mut program,
                FrameParameters {
                    model: &model,
                    gain: index,
                    previous_gain: 10,
                    long: frame_size == 320,
                    bandwidth: Bandwidth::Wideband,
                    pitch: None,
                    rate: 16000,
                },
            )
            .unwrap();
            encode_pulses(&mut program, &quantized.0, false).unwrap();
            choices.push(program);
            semantics.push((
                model.stage1,
                model.residual.clone(),
                index,
                quantized.0.clone(),
            ));
        }
        programs.push(choices);
        semantic_channels.push(semantics);
    }
    let mut packets = Vec::new();
    let mut semantic_packets = Vec::new();
    for mid in 0..2 {
        for side in 0..2 {
            let mut coder = RangeEncoder::new(512).unwrap();
            encode_header(&mut coder, 2).unwrap();
            programs[0][mid].replay(&mut coder).unwrap();
            programs[1][side].replay(&mut coder).unwrap();
            packets.push(coder.finish().unwrap());
            semantic_packets.push(vec![
                semantic_channels[0][mid].clone(),
                semantic_channels[1][side].clone(),
            ]);
        }
    }
    (packets, semantic_packets)
}

#[cfg(test)]
#[test]
fn indexed_trial_cache_order_and_domain() {
    let mut indexed = TrialCache::default();
    let mut expected = BTreeMap::new();
    for model in [2, 0, 1] {
        for gain in (0..64).rev() {
            for pressure in (0..13).rev() {
                for shaping in [true, false] {
                    let key = (model, gain, pressure, shaping);
                    let frame = Rc::new((vec![i16::try_from(gain).unwrap()], [0; 16], vec![1]));
                    let error = i128::from(gain) * 13 + i128::from(pressure);
                    indexed.insert(key, frame.clone(), error);
                    expected.insert(key, (frame, error));
                }
            }
        }
    }
    assert_eq!(
        indexed.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
        expected.keys().copied().collect::<Vec<_>>()
    );
    for (key, (frame, error)) in expected {
        let entry = indexed.entry(key).unwrap();
        assert_eq!(*entry.frame, *frame);
        assert_eq!(entry.distortion, error);
    }
    assert!(std::panic::catch_unwind(|| TrialCache::slot((0, 64, 0, false))).is_err());
    assert!(std::panic::catch_unwind(|| TrialCache::slot((0, 0, 13, false))).is_err());
}

#[cfg(test)]
#[test]
#[allow(clippy::too_many_lines)] // Identical frame streams compare both storage implementations and persistent state.
fn indexed_trial_cache_stream_parity() {
    struct TreeScope(bool);
    impl TreeScope {
        fn enter() -> Self {
            Self(TREE_TRIAL_CACHE.replace(true))
        }
    }
    impl Drop for TreeScope {
        fn drop(&mut self) {
            TREE_TRIAL_CACHE.set(self.0);
        }
    }
    for (rate, channels, size, bitrate) in [
        (SampleRate::Hz8000, crate::Channels::Mono, 80, 16000),
        (SampleRate::Hz12000, crate::Channels::Stereo, 120, 16000),
        (SampleRate::Hz16000, crate::Channels::Stereo, 160, 16000),
        (SampleRate::Hz16000, crate::Channels::Stereo, 160, 32000),
        (SampleRate::Hz16000, crate::Channels::Stereo, 320, 48000),
        (SampleRate::Hz16000, crate::Channels::Mono, 320, 128_000),
    ] {
        let config = EncoderConfig {
            sample_rate: rate,
            channels,
            frame_size: size,
            bitrate,
        };
        let mut indexed = SilkEncoder::new(config).unwrap();
        let mut tree = indexed.clone();
        let mut pcm_a = crate::Decoder::new(rate, channels).unwrap();
        let mut pcm_b = pcm_a.clone();
        for frame in 0..4 {
            let input: Vec<f32> = (0..size)
                .flat_map(|i| {
                    let scale = if frame == 0 {
                        0.0
                    } else if frame == 2 {
                        0.001
                    } else {
                        0.1
                    };
                    let x = (std::f32::consts::TAU * 173.0 * (i + frame * size) as f32
                        / rate as u32 as f32)
                        .sin()
                        * scale;
                    [x, -x * 0.6].into_iter().take(channels as usize)
                })
                .collect();
            let actual = indexed.encode(&input);
            let expected = {
                let _scope = TreeScope::enter();
                tree.encode(&input)
            };
            match (actual, expected) {
                (Ok(actual), Ok(expected)) => {
                    assert_eq!(actual, expected, "{config:?} frame={frame}");
                    let mut a = vec![0; size * channels as usize];
                    let mut b = a.clone();
                    assert_eq!(
                        pcm_a.decode(Some(&actual), &mut a, false).unwrap(),
                        pcm_b.decode(Some(&expected), &mut b, false).unwrap()
                    );
                    assert_eq!(a, b);
                }
                (Err(actual), Err(expected)) => {
                    assert_eq!(actual.to_string(), expected.to_string());
                }
                result => panic!("storage result mismatch {result:?}"),
            }
            assert_eq!(indexed.history, tree.history);
            assert_eq!(indexed.gain, tree.gain);
            assert_eq!(indexed.previous_index, tree.previous_index);
            assert_eq!(indexed.pcm_history, tree.pcm_history);
        }
    }
    for rate in [SampleRate::Hz24000, SampleRate::Hz48000] {
        let config = EncoderConfig {
            sample_rate: rate,
            channels: crate::Channels::Stereo,
            frame_size: rate as usize / 100,
            bitrate: 16000,
        };
        let mut indexed = super::Encoder::new_silk(config).unwrap();
        let mut tree = indexed.clone();
        for frame in 0..3 {
            let input: Vec<_> = (0..config.frame_size * 2)
                .map(|i| ((i * 37 + frame * 11) % 997) as f32 / 65536.0)
                .collect();
            let actual = indexed.encode(&input).unwrap();
            let expected = {
                let _scope = TreeScope::enter();
                tree.encode(&input).unwrap()
            };
            assert_eq!(actual, expected);
        }
        let actual = indexed.finish().unwrap();
        let expected = {
            let _scope = TreeScope::enter();
            tree.finish().unwrap()
        };
        assert_eq!(actual, expected);
    }
}

fn shell(enc: &mut impl SilkEntropy, magnitudes: &[u16]) -> Result<()> {
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
