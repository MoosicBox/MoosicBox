//! Integer hybrid analysis with decoder-output phase compensation.
//! The SILK interpolation filter is dispersive near the band edge. A finite
//! inverse-response analysis filter aligns its output with CELT, rather than
//! treating frequency-dependent phase as a constant sample delay.
use crate::{Channels, Result};

/// Packet-domain layout shared by main hybrid and redundant CELT coding.
/// This describes wire syntax, not qualification or permission to encode it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Coordinator-owned mod.rs integrates this interface.
pub(super) struct CodingLayout {
    pub end_band: usize,
    pub hybrid_toc: u8,
    pub celt_toc: u8,
}

/// Select matching bandwidth, duration and channel flags for both layers.
///
/// # Errors
/// * Rejects bandwidths other than SWB/FB and durations other than 10/20 ms.
#[allow(dead_code)] // Coordinator-owned mod.rs integrates this interface.
pub(super) fn coding_layout(
    bandwidth: crate::Bandwidth,
    channels: Channels,
    frame_size_48k: usize,
) -> Result<CodingLayout> {
    let (end_band, hybrid_base, celt_base) = match bandwidth {
        crate::Bandwidth::SuperWideband => (19, 0x60, 0xc0),
        crate::Bandwidth::Fullband => (21, 0x70, 0xe0),
        _ => {
            return Err(crate::Error::Unsupported(
                "hybrid layout requires SWB or FB".into(),
            ));
        }
    };
    let lm = match frame_size_48k {
        480 => 2,
        960 => 3,
        _ => {
            return Err(crate::Error::Unsupported(
                "hybrid layout requires 10/20 ms".into(),
            ));
        }
    };
    let stereo = if channels == Channels::Stereo { 4 } else { 0 };
    Ok(CodingLayout {
        end_band,
        hybrid_toc: hybrid_base | (u8::from(lm == 3) << 3) | stereo,
        celt_toc: celt_base | (lm << 3) | stereo,
    })
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn hybrid_and_celt_layouts_match_wire_bandwidth_and_channels() {
        for channels in [Channels::Mono, Channels::Stereo] {
            let stereo = if channels == Channels::Stereo { 4 } else { 0 };
            for (bandwidth, end, hybrid, celt) in [
                (crate::Bandwidth::SuperWideband, 19, 0x60, 0xc0),
                (crate::Bandwidth::Fullband, 21, 0x70, 0xe0),
            ] {
                for (size, duration, lm) in [(480, 0, 2), (960, 8, 3)] {
                    let layout = coding_layout(bandwidth, channels, size).unwrap();
                    assert_eq!(layout.end_band, end);
                    assert_eq!(layout.hybrid_toc, hybrid | duration | stereo);
                    assert_eq!(layout.celt_toc, celt | (lm << 3) | stereo);
                }
            }
        }
        assert!(coding_layout(crate::Bandwidth::Wideband, Channels::Mono, 480).is_err());
        assert!(coding_layout(crate::Bandwidth::Fullband, Channels::Stereo, 240).is_err());
    }
}

/// Candidate split reserving CELT header/shape space before SILK searches its
/// excitation budget. This is not a public supported-bitrate declaration.
#[cfg(test)]
pub(super) fn candidate_lowband_bitrate(
    bitrate: u32,
    channels: Channels,
    frame_size: usize,
) -> u32 {
    let packet_bytes = bitrate as usize * frame_size / 384_000;
    let reserve = if channels == Channels::Stereo { 12 } else { 8 };
    let silk_bytes =
        (packet_bytes.saturating_sub(1) * 2 / 3).min(packet_bytes.saturating_sub(1 + reserve));
    // SILK's budget includes its own nominal TOC byte, although its shared
    // prefix has no TOC. Account for that subtraction exactly once.
    (((silk_bytes + 1) * 384_000 / frame_size) as u32).clamp(8_000, 128_000)
}

/// Candidate low-band-first allocation for the four-band hybrid CELT suffix.
/// Reserve CELT's minimum entropy/shape space, then give SILK the remaining
/// budget: the measured 2/3 split starves low-band reconstruction at 32/64k.
/// Test-only until independent content qualification establishes a floor.
#[cfg(test)]
pub(super) fn candidate_lowband_first_bitrate(bitrate: u32, frame_size: usize) -> u32 {
    let payload = (bitrate as usize * frame_size / 384_000).saturating_sub(1);
    let silk_bytes = (payload * 7 / 8).min(payload.saturating_sub(8));
    (((silk_bytes + 1) * 384_000 / frame_size) as u32).clamp(8_000, 128_000)
}

/// Experimental short-packet split reclaiming two bytes from the previous
/// eight-byte suffix reserve. The existing suffix encoder remains responsible
/// for entropy fit; this is not a minimum legal CELT budget or supported policy.
#[cfg(test)]
pub(super) fn candidate_short_prefix_bitrate(bitrate: u32, frame_size: usize) -> u32 {
    let payload = (bitrate as usize * frame_size / 384_000).saturating_sub(1);
    let silk_bytes = payload.saturating_sub(6);
    (((silk_bytes + 1) * 384_000 / frame_size) as u32).clamp(8_000, 128_000)
}

#[cfg(test)]
std::thread_local! {
    /// Scoped to the explicitly ignored frontier experiment's encoding thread.
    pub(super) static PREFIX_FRONTIER_BYTES: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Frame-budget allocation for practical stereo hybrid packets. The low-rate
/// endpoint preserves the qualified seven-eighths prefix with eight-byte suffix
/// reserve; the extra prefix share tapers to the established split at 64 kbit/s.
/// Mono and 128 kbit/s or higher retain their established allocation. This helper
/// allocates bits only: constructor guards remain responsible for supported rates.
pub(super) fn packet_lowband_bitrate(
    bitrate: u32,
    channels: Channels,
    frame_size_48k: usize,
) -> u32 {
    #[cfg(test)]
    if bitrate == 32_000
        && channels == Channels::Stereo
        && frame_size_48k == 480
        && let Some(bytes) = PREFIX_FRONTIER_BYTES.get()
    {
        return ((bytes + 1) * 384_000 / frame_size_48k) as u32;
    }
    let established = lowband_bitrate(bitrate, channels);
    if channels == Channels::Mono || bitrate >= 64_000 {
        return established;
    }
    let payload = (bitrate as usize * frame_size_48k / 384_000).saturating_sub(1);
    let prefix = (payload * 7 / 8).min(payload.saturating_sub(8));
    let low_first = (((prefix + 1) * 384_000 / frame_size_48k) as u32).clamp(8_000, 128_000);
    established + low_first.saturating_sub(established) * (64_000 - bitrate).min(32_000) / 32_000
}

/// Append the low-band prefix without changing the established high-rate path.
/// Practical-rate candidates rank fitting reconstructions before committing
/// either predictive history or entropy operations to the shared packet.
pub(super) fn encode_lowband_prefix(
    silk: &mut super::silk::SilkEncoder,
    pcm: &[f32],
    encoder: &mut crate::range::RangeEncoder,
    packet_bitrate: u32,
) -> Result<()> {
    if packet_bitrate < 128_000 {
        silk.encode_prefix_ranked(pcm, encoder)
    } else {
        silk.encode_prefix(pcm, encoder)
    }
}

pub(super) const fn lowband_bitrate(bitrate: u32, channels: Channels) -> u32 {
    match channels {
        Channels::Mono => bitrate / 2,
        Channels::Stereo => {
            let value = bitrate * 3 / 4;
            if value > 128_000 { 128_000 } else { value }
        }
    }
}
use crate::celt::fixed_point::celt_cos_norm;
use crate::silk::resampler::SilkResampler;

pub(super) const DELAY: usize = 384;
const FFT: usize = 1024;
const TAPS: usize = 769;

#[derive(Clone)]
pub(super) struct PracticalSynthesis {
    pub decoder: crate::Decoder,
    delay: Vec<i16>,
    #[cfg(test)]
    pub pending_prefix: Option<Box<(NativePrefixScore, crate::Decoder, crate::Decoder)>>,
}

impl PracticalSynthesis {
    #[cfg(test)]
    pub(super) fn new() -> Result<Self> {
        Self::with_layout(crate::SampleRate::Hz48000, Channels::Stereo, DELAY)
    }

    pub(super) fn with_layout(
        rate: crate::SampleRate,
        channels: Channels,
        lookahead: usize,
    ) -> Result<Self> {
        Ok(Self {
            decoder: crate::Decoder::new(rate, channels)?,
            delay: vec![0; lookahead * super::channel_count(channels)],
            #[cfg(test)]
            pending_prefix: None,
        })
    }

    /// Advance the fixed 384-sample target delay exactly once per input frame.
    pub(super) fn target(&mut self, pcm: &[f32]) -> Vec<i16> {
        self.delay.extend(
            pcm.iter()
                .map(|x| (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16),
        );
        self.delay.drain(..pcm.len()).collect()
    }
}

// One holder represents one immutable incoming synthesis state and target.
// Configuration is explicit; replay bytes are compared exactly, never hashed.
// Keys losslessly encode a zero suffix as the ORIGINAL payload length plus
// the prefix through its last nonzero byte. This is injective for arbitrary
// payloads, including raw-tail bits. Decoder input is NEVER shortened.
type ReplayScores = std::collections::BTreeMap<
    (u32, usize, usize, usize),
    std::collections::BTreeMap<Vec<u8>, i128>,
>;

fn replay_key_prefix(replay: &[u8]) -> &[u8] {
    // Replay is forward arithmetic data followed by zero fill. Skip padding
    // a word at a time, retaining exact identity including arbitrary raw tails.
    let mut length = replay.len();
    while length >= 8 && replay[length - 8..length] == [0; 8] {
        length -= 8;
    }
    while length > 0 && replay[length - 1] == 0 {
        length -= 1;
    }
    &replay[..length]
}

/// Costs of the production scorer path, collected only by explicit unit tests.
/// Keep timing outside the arithmetic and winner ordering being measured.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct NativeScoreCosts {
    misses: usize,
    decoder_clones: usize,
    pcm_allocations: usize,
    pcm_samples: usize,
    replay_bytes: usize,
    clone_ns: u128,
    decode_ns: u128,
    distortion_ns: u128,
    key_ns: u128,
    cache_ns: u128,
    callback_ns: u128,
    proposal_calls: usize,
    proposal_ns: u128,
    typed_calls: usize,
    typed_ns: u128,
    winner_copies: usize,
    winner_bytes: usize,
}

#[cfg_attr(not(test), derive(Default))]
pub(super) struct NativeScoreCache {
    scores: ReplayScores,
    // Workspace is never semantic cache state. Clone deliberately drops it:
    // encoder speculation must not copy the inactive CELT decoder or PCM buffer.
    scratch: Option<Box<crate::Decoder>>,
    pcm: Vec<i16>,
    #[cfg(test)]
    costs: NativeScoreCosts,
    #[cfg(test)]
    pub calls: usize,
    #[cfg(test)]
    pub hits: usize,
    #[cfg(test)]
    pub disabled: bool,
}

impl Clone for NativeScoreCache {
    fn clone(&self) -> Self {
        Self {
            scores: self.scores.clone(),
            scratch: None,
            pcm: Vec::new(),
            #[cfg(test)]
            costs: self.costs.clone(),
            #[cfg(test)]
            calls: self.calls,
            #[cfg(test)]
            hits: self.hits,
            #[cfg(test)]
            disabled: self.disabled,
        }
    }
}

#[cfg(test)]
impl Default for NativeScoreCache {
    fn default() -> Self {
        Self {
            scores: ReplayScores::new(),
            scratch: None,
            pcm: Vec::new(),
            costs: NativeScoreCosts::default(),
            #[cfg(test)]
            calls: 0,
            #[cfg(test)]
            hits: 0,
            #[cfg(test)]
            disabled: std::env::var_os("MOOSICBOX_OPUS_TEST_DISABLE_NATIVE_CACHE").is_some(),
        }
    }
}

#[cfg(test)]
impl NativeScoreCache {
    #[cfg(test)]
    pub(super) fn report_costs(&self) {
        if let Some(decoder) = &self.scratch {
            eprintln!(
                "RD_CHANNEL_REUSE calls_hits_syntheses_retained={:?}",
                decoder.silk.channel_reuse_counts()
            );
            eprintln!(
                "RD_COEFFICIENT hits_misses_retained={:?}",
                decoder.silk.coefficient_memo_counts()
            );
        }
        eprintln!(
            "RD_CALLBACK calls={} hits={} costs={:?}",
            self.calls, self.hits, self.costs
        );
    }

    pub(super) fn sizes(&self) -> (usize, usize) {
        (
            self.scores
                .values()
                .map(std::collections::BTreeMap::len)
                .sum(),
            self.scores
                .values()
                .flat_map(std::collections::BTreeMap::keys)
                .map(Vec::len)
                .sum(),
        )
    }
}

#[cfg(test)]
std::thread_local! {
    pub(super) static POOL_DIAGNOSE_SKIP: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static POOL_DIAGNOSE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static POOL48: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static POOL48_EVALUATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static REFERENCE_CACHE_LIFECYCLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Clone)]
pub(super) struct NativePrefixScore {
    pub incoming: crate::Decoder,
    pub target: Vec<i16>,
    pub cache: NativeScoreCache,
    pub best: Option<(i128, Vec<u8>, super::EncoderConfig)>,
}

impl NativePrefixScore {
    #[cfg(test)]
    pub(super) fn decode(
        &self,
        decoder: &mut crate::Decoder,
        config: super::EncoderConfig,
        bytes: &[u8],
    ) -> Result<(Vec<i16>, u32)> {
        let mut pcm = vec![0; self.target.len()];
        let tell = Self::decode_into(decoder, config, bytes, &mut pcm)?;
        Ok((pcm, tell))
    }

    fn decode_into(
        decoder: &mut crate::Decoder,
        config: super::EncoderConfig,
        bytes: &[u8],
        pcm: &mut [i16],
    ) -> Result<u32> {
        let mut range = crate::range::RangeDecoder::new(bytes)?;
        let layout = crate::Configuration {
            mode: crate::OpusMode::SilkOnly,
            bandwidth: crate::Bandwidth::Wideband,
            frame_size: if config.frame_size == 160 {
                crate::FrameSize::Ms10
            } else {
                crate::FrameSize::Ms20
            },
        };
        let samples = decoder.decode_silk_payload(&mut range, layout, config.channels, pcm)?;
        assert_eq!(samples * super::channel_count(decoder.channels), pcm.len());
        Ok(range.ec_tell())
    }

    /// Restore exactly the domain touched by `decode_silk_payload_inner` and
    /// its configure/resample helpers. CELT, packet-mode bookkeeping and gain
    /// application are not read or advanced by that internal entry point.
    /// The actual entropy, prediction and resampling arithmetic stays there.
    fn restore_silk(incoming: &crate::Decoder, trial: &mut crate::Decoder) {
        trial.sample_rate = incoming.sample_rate;
        trial.channels = incoming.channels;
        trial
            .silk
            .restore_prediction_with_coefficients(&incoming.silk);
        trial.silk_delay_samples = incoming.silk_delay_samples;
        trial.silk_configured_rate = incoming.silk_configured_rate;
        trial.silk_configured_channels = incoming.silk_configured_channels;
        trial.silk_configured_frame_ms = incoming.silk_configured_frame_ms;
        trial.silk_resamplers.clone_from(&incoming.silk_resamplers);
        trial.silk_resampler_input_rate = incoming.silk_resampler_input_rate;
    }

    #[cfg(test)]
    pub fn branches(&self, bytes: &[u8]) -> Result<(crate::Decoder, crate::Decoder)> {
        let (_, replay, selected) = self.best.as_ref().expect("scored prefix selected");
        let mut actual = self.incoming.clone();
        let mut expected = self.incoming.clone();
        assert_eq!(
            self.decode(&mut actual, *selected, bytes)?,
            self.decode(&mut expected, *selected, replay)?
        );
        Ok((actual, expected))
    }

    #[cfg(test)]
    pub fn verify(&self, bytes: &[u8]) -> Result<()> {
        let (_, replay, selected) = self.best.as_ref().expect("scored prefix selected");
        let config = *selected;
        let mut actual = self.incoming.clone();
        let mut expected = self.incoming.clone();
        let a = self.decode(&mut actual, config, bytes)?;
        let b = self.decode(&mut expected, config, replay)?;
        assert_eq!(a, b, "actual Hybrid SILK prefix/replay PCM and entropy");
        // Decode a following legal prefix from the two resulting states. This
        // detects divergence in predictor, mono delay and resampler history.
        assert_eq!(
            self.decode(&mut actual, config, replay)?,
            self.decode(&mut expected, config, replay)?,
            "following prefix synthesis"
        );
        Ok(())
    }
}

impl super::silk::PrefixCandidateScorer for NativePrefixScore {
    #[cfg(test)]
    fn proposal_score(
        &mut self,
        config: super::EncoderConfig,
        mid: &[i16],
        side: &[i16],
    ) -> Result<Option<i128>> {
        let started = std::time::Instant::now();
        if self.cache.scratch.is_none() {
            self.cache.scratch = Some(Box::new(self.incoming.clone()));
        }
        let decoder = self.cache.scratch.as_mut().expect("projection workspace");
        Self::restore_silk(&self.incoming, decoder);
        decoder.configure_silk(
            crate::SampleRate::Hz16000,
            config.channels,
            if config.frame_size == 160 { 10 } else { 20 },
        )?;
        let raw = decoder.silk.project_zero_weight_stereo(mid, side)?;
        let synthesis =
            decoder.resample_silk(&raw, 16000, decoder.sample_rate as u32, config.channels)?;
        let error = synthesis
            .iter()
            .zip(&self.target)
            .map(|(&a, &b)| (i128::from(a) - i128::from(b)).pow(2))
            .sum();
        self.cache.costs.proposal_calls += 1;
        self.cache.costs.proposal_ns += started.elapsed().as_nanos();
        Ok(Some(error))
    }

    #[cfg(test)]
    fn diagnostic_fork(&self) -> Option<Box<dyn super::silk::PrefixCandidateScorer>> {
        Some(Box::new(Self {
            incoming: self.incoming.clone(),
            target: self.target.clone(),
            cache: NativeScoreCache::default(),
            best: None,
        }))
    }

    #[allow(clippy::too_many_lines)] // Test-only phase counters surround unchanged scoring order.
    fn score(&mut self, config: super::EncoderConfig, replay: &[u8]) -> Result<i128> {
        #[cfg(test)]
        let callback_started = std::time::Instant::now();
        #[cfg(test)]
        let semantic_channels = super::silk::take_semantic_channels();
        let key = (
            config.sample_rate as u32,
            super::channel_count(config.channels),
            config.frame_size,
            replay.len(),
        );
        #[cfg(test)]
        let key_started = std::time::Instant::now();
        let stored = replay_key_prefix(replay);
        #[cfg(test)]
        {
            self.cache.costs.key_ns += key_started.elapsed().as_nanos();
            self.cache.calls += 1;
        }
        #[cfg(test)]
        let cache_started = std::time::Instant::now();
        let cached = self
            .cache
            .scores
            .get(&key)
            .and_then(|scores| scores.get(stored))
            .copied();
        #[cfg(test)]
        {
            self.cache.costs.cache_ns += cache_started.elapsed().as_nanos();
        }
        let enabled = true;
        #[cfg(test)]
        let enabled = enabled && !self.cache.disabled;
        let error = if let Some(error) = cached.filter(|_| enabled) {
            #[cfg(test)]
            {
                self.cache.hits += 1;
            }
            error
        } else {
            #[cfg(test)]
            let clone_started = std::time::Instant::now();
            if self.cache.scratch.is_none() {
                self.cache.scratch = Some(Box::new(self.incoming.clone()));
                #[cfg(test)]
                {
                    self.cache.costs.decoder_clones += 1;
                }
            }
            let decoder = self
                .cache
                .scratch
                .as_mut()
                .expect("initialized scorer workspace");
            Self::restore_silk(&self.incoming, decoder);
            #[cfg(test)]
            {
                self.cache.costs.misses += 1;
                self.cache.costs.clone_ns += clone_started.elapsed().as_nanos();
                self.cache.costs.replay_bytes += replay.len();
                self.cache.costs.pcm_allocations +=
                    usize::from(self.cache.pcm.capacity() < self.target.len());
                self.cache.costs.pcm_samples += self.target.len();
            }
            self.cache.pcm.resize(self.target.len(), 0);
            self.cache.pcm.fill(0);
            #[cfg(test)]
            let decode_started = std::time::Instant::now();
            #[cfg(test)]
            let typed = super::silk::TYPED_SCORE.get() && semantic_channels.len() == 2;
            #[cfg(test)]
            if typed {
                decoder.configure_silk(
                    crate::SampleRate::Hz16000,
                    config.channels,
                    if config.frame_size == 160 { 10 } else { 20 },
                )?;
                let raw = decoder.silk.typed_unvoiced_prefix(
                    &semantic_channels,
                    decoder.channels == Channels::Mono,
                )?;
                let synthesis = decoder.resample_silk(
                    &raw,
                    16000,
                    decoder.sample_rate as u32,
                    decoder.channels,
                )?;
                assert_eq!(synthesis.len(), self.cache.pcm.len());
                self.cache.pcm.copy_from_slice(&synthesis);
                self.cache.costs.typed_calls += 1;
                self.cache.costs.typed_ns += decode_started.elapsed().as_nanos();
            } else {
                Self::decode_into(decoder, config, replay, &mut self.cache.pcm)?;
            }
            #[cfg(not(test))]
            Self::decode_into(decoder, config, replay, &mut self.cache.pcm)?;
            #[cfg(test)]
            {
                self.cache.costs.decode_ns += decode_started.elapsed().as_nanos();
            }
            #[cfg(test)]
            let distortion_started = std::time::Instant::now();
            // At most 5760 stereo samples, each difference <= 65535: the
            // exact integer sum fits i64. Avoid i128 multiply in the hot loop;
            // promotion after summation preserves every score and strict tie.
            let error = i128::from(
                self.cache
                    .pcm
                    .iter()
                    .zip(&self.target)
                    .map(|(&a, &b)| {
                        let difference = i64::from(a) - i64::from(b);
                        difference * difference
                    })
                    .sum::<i64>(),
            );
            #[cfg(test)]
            {
                self.cache.costs.distortion_ns += distortion_started.elapsed().as_nanos();
            }
            if enabled {
                // Frame-local retained keys are compact but arbitrary callers
                // can provide long nonzero payloads. Bound bytes as well as
                // entries; eviction changes cost only, never scoring/order.
                let entries: usize = self
                    .cache
                    .scores
                    .values()
                    .map(std::collections::BTreeMap::len)
                    .sum();
                if entries >= 4096 {
                    self.cache.scores.clear();
                }
                if stored.len() <= 1275 {
                    self.cache
                        .scores
                        .entry(key)
                        .or_default()
                        .insert(stored.to_vec(), error);
                }
            }
            error
        };
        if self.best.as_ref().is_none_or(|old| error < old.0) {
            #[cfg(test)]
            {
                self.cache.costs.winner_copies += 1;
                self.cache.costs.winner_bytes += replay.len();
            }
            self.best = Some((error, replay.to_vec(), config));
        }
        #[cfg(test)]
        {
            self.cache.costs.callback_ns += callback_started.elapsed().as_nanos();
        }
        Ok(error)
    }
}

#[derive(Clone)]
pub(super) struct LowBand {
    history: Vec<Vec<i32>>,
    coefficients: Vec<i32>,
    celt_delay: Vec<Vec<f32>>,
}

fn cosine(phase: i32) -> i64 {
    i64::from(celt_cos_norm(phase))
}

impl LowBand {
    pub(super) fn new(channels: usize) -> Result<Self> {
        // Characterize the existing integer reference output filter, not a
        // platform codec. The leading input zero includes SILK's mono/MS delay.
        let mut resampler = SilkResampler::new(16000, 48000)?;
        let mut impulse = vec![0_i16; 352];
        impulse[1] = 16384;
        let response = resampler.process(&impulse)?;
        let mut frequencies = Vec::new();
        for frequency in 0..=FFT / 6 {
            let (mut real, mut imaginary) = (0_i64, 0_i64);
            for (i, &sample) in response.iter().take(FFT).enumerate() {
                let phase = ((frequency * i) % FFT) as i32 * 128;
                real += i64::from(sample) * cosine(phase);
                imaginary -= i64::from(sample) * cosine(32768 - phase);
            }
            real >>= 15;
            imaginary >>= 15;
            let norm =
                i128::from(real) * i128::from(real) + i128::from(imaginary) * i128::from(imaginary);
            let taper = if frequency < 164 {
                32768
            } else {
                (FFT / 6 - frequency) as i64 * 32768 / 7
            };
            let numerator = 3_i128 * 16384 * i128::from(taper);
            let inverse_real = (numerator * i128::from(real) / norm.max(1)) as i64;
            let inverse_imaginary = (-numerator * i128::from(imaginary) / norm.max(1)) as i64;
            frequencies.push((inverse_real, inverse_imaginary));
        }
        let mut coefficients = Vec::with_capacity(TAPS);
        for tap in 0..TAPS {
            let mut value = frequencies[0].0;
            for (frequency, &(real, imaginary)) in frequencies.iter().enumerate().skip(1) {
                let phase =
                    ((frequency as i64 * (tap as i64 - DELAY as i64) * 128) % 131_072) as i32;
                value += 2 * ((real * cosine(phase) - imaginary * cosine(32768 - phase)) >> 15);
            }
            coefficients.push((value / FFT as i64) as i32);
        }
        let correction = 32768 - coefficients.iter().sum::<i32>();
        coefficients[DELAY] += correction;
        Ok(Self {
            history: vec![vec![0; TAPS]; channels],
            coefficients,
            celt_delay: vec![vec![0.0; DELAY - 120]; channels],
        })
    }

    pub(super) fn delay_celt(&mut self, pcm: &[f32]) -> Vec<f32> {
        let channels = self.history.len();
        let mut output = Vec::with_capacity(pcm.len());
        for frame in pcm.chunks_exact(channels) {
            for (channel, &sample) in frame.iter().enumerate() {
                output.push(self.celt_delay[channel][0]);
                self.celt_delay[channel].rotate_left(1);
                self.celt_delay[channel][DELAY - 121] = sample;
            }
        }
        output
    }

    pub(super) fn process(&mut self, pcm: &[f32]) -> Vec<f32> {
        let channels = self.history.len();
        let mut output = Vec::with_capacity(pcm.len() / 3);
        for (i, frame) in pcm.chunks_exact(channels).enumerate() {
            for (channel, &sample) in frame.iter().enumerate() {
                self.history[channel].rotate_right(1);
                self.history[channel][0] = (f64::from(sample) * 134_217_728.0).round() as i32;
            }
            if i % 3 == 0 {
                for history in &self.history {
                    let value = (history
                        .iter()
                        .zip(&self.coefficients)
                        .map(|(&x, &c)| i64::from(x) * i64::from(c))
                        .sum::<i64>()
                        >> 15) as i32;
                    output.push((f64::from(value) / 134_217_728.0) as f32);
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod cache_key_tests {
    use super::{NativePrefixScore, NativeScoreCache, replay_key_prefix};

    /// Production search and a whole-Decoder-clone baseline see every callback
    /// in exactly the same order. Timings are evidence, never pass thresholds.
    struct ProfileScore {
        native: NativePrefixScore,
        baseline: bool,
    }

    impl crate::encoder::silk::PrefixCandidateScorer for ProfileScore {
        fn score(&mut self, config: crate::EncoderConfig, replay: &[u8]) -> crate::Result<i128> {
            if !self.baseline {
                return self.native.score(config, replay);
            }
            let started = std::time::Instant::now();
            let mut decoder = self.native.incoming.clone();
            self.native.cache.costs.clone_ns += started.elapsed().as_nanos();
            self.native.cache.costs.decoder_clones += 1;
            self.native.cache.calls += 1;
            let started = std::time::Instant::now();
            let (pcm, _) = self.native.decode(&mut decoder, config, replay)?;
            self.native.cache.costs.decode_ns += started.elapsed().as_nanos();
            let error = pcm
                .iter()
                .zip(&self.native.target)
                .map(|(&a, &b)| (i128::from(a) - i128::from(b)).pow(2))
                .sum();
            if self.native.best.as_ref().is_none_or(|old| error < old.0) {
                self.native.best = Some((error, replay.to_vec(), config));
            }
            Ok(error)
        }
    }

    #[test]
    #[ignore = "explicit production scorer cost profile; no C or acceptance thresholds"]
    #[allow(clippy::too_many_lines)] // Bounded old/new production search and phase evidence in one cell.
    fn native_prefix_scorer_cost_profile() {
        let directory = std::env::var("MOOSICBOX_OPUS_QUALIFICATION_MEDIA").unwrap();
        for (label, channels, bitrate, scale, file, rate) in [
            (
                "music24stereo10",
                crate::Channels::Stereo,
                24_000,
                1.0_f32,
                "music-carefree-30s.wav",
                48_000,
            ),
            (
                "quiet48mono10",
                crate::Channels::Mono,
                48_000,
                0.01,
                "speech-osr-8k.wav",
                8_000,
            ),
        ] {
            let count = channels as usize;
            let config = crate::EncoderConfig {
                sample_rate: crate::SampleRate::Hz16000,
                channels,
                bitrate,
                frame_size: 160,
            };
            // Read the reviewed WAV at a fixed non-silent offset; no media
            // generation/download or substitute corpus is allowed here.
            let read = std::process::Command::new("python3")
                .args(["-B", "-c", "import sys,wave; w=wave.open(sys.argv[1]); w.setpos(w.getframerate()*2); sys.stdout.buffer.write(w.readframes(w.getframerate()//100))"])
                .arg(std::path::Path::new(&directory).join(file)).output().unwrap();
            assert!(read.status.success());
            let source: Vec<i16> = read
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|x| i16::from_le_bytes(*x))
                .collect();
            let input = if rate == 48_000 {
                let mut converter = crate::encoder::resample::SilkInput::new(count, rate).unwrap();
                converter
                    .process(
                        &source
                            .iter()
                            .map(|&x| f32::from(x) / 32768.0 * scale)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap()
            } else {
                let mut converter =
                    crate::silk::resampler::SilkResampler::new(rate, 16_000).unwrap();
                converter
                    .process(&source)
                    .unwrap()
                    .into_iter()
                    .map(|x| f32::from(x) / 32768.0 * scale)
                    .collect()
            };
            let target: Vec<i16> = input
                .chunks_exact(count)
                .flat_map(|frame| {
                    (0..3).flat_map(move |_| frame.iter().map(|&x| (x * 32768.0).round() as i16))
                })
                .collect();
            let make = |baseline| ProfileScore {
                baseline,
                native: NativePrefixScore {
                    incoming: crate::Decoder::new(crate::SampleRate::Hz48000, channels).unwrap(),
                    target: target.clone(),
                    cache: NativeScoreCache::default(),
                    best: None,
                },
            };
            let mut optimized = make(false);
            let mut baseline = make(true);
            let mut a = crate::encoder::silk::SilkEncoder::new(config).unwrap();
            let mut b = a.clone();
            let mut range_a = crate::range::RangeEncoder::new(512).unwrap();
            let mut range_b = range_a.clone();
            let started = std::time::Instant::now();
            a.encode_prefix_scored(&input, &mut range_a, &mut optimized)
                .unwrap();
            let optimized_ns = started.elapsed().as_nanos();
            let started = std::time::Instant::now();
            b.encode_prefix_scored(&input, &mut range_b, &mut baseline)
                .unwrap();
            let baseline_ns = started.elapsed().as_nanos();
            assert_eq!(range_a.finish().unwrap(), range_b.finish().unwrap());
            let best = optimized.native.best.as_ref().unwrap();
            let old = baseline.native.best.as_ref().unwrap();
            assert_eq!(best.0, old.0);
            assert_eq!(best.1, old.1);
            assert_eq!(optimized.native.cache.calls, baseline.native.cache.calls);
            assert_eq!(optimized.native.cache.costs.decoder_clones, 1);
            assert_eq!(optimized.native.cache.costs.pcm_allocations, 1);
            let (unique, key_bytes) = optimized.native.cache.sizes();
            let outside_callback_ns =
                optimized_ns.saturating_sub(optimized.native.cache.costs.callback_ns);
            eprintln!(
                "NATIVE_SCORE_COST case={label} total_ns={optimized_ns} baseline_ns={baseline_ns} outside_callback_ns={outside_callback_ns} unique={unique} key_bytes={key_bytes} calls={} hits={} optimized={:?} baseline={:?}",
                optimized.native.cache.calls,
                optimized.native.cache.hits,
                optimized.native.cache.costs,
                baseline.native.cache.costs
            );
        }
    }

    #[test]
    fn silk_domain_restore_matches_full_clone_through_success_and_error() {
        for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
            let config = crate::EncoderConfig {
                sample_rate: crate::SampleRate::Hz16000,
                channels,
                bitrate: 48000,
                frame_size: 320,
            };
            let packet: &[u8] = if channels == crate::Channels::Mono {
                include_bytes!("../silk/reference/wb_sequence_0.packet")
            } else {
                include_bytes!("../silk/reference/wb_mixed.packet")
            };
            let mut incoming = crate::Decoder::new(crate::SampleRate::Hz48000, channels).unwrap();
            for _ in 0..3 {
                incoming
                    .decode(Some(packet), &mut vec![0; 960 * channels as usize], false)
                    .unwrap();
            }
            let scorer = NativePrefixScore {
                incoming: incoming.clone(),
                target: vec![0; 960 * channels as usize],
                cache: NativeScoreCache::default(),
                best: None,
            };
            let mut trial = incoming.clone();
            for bytes in [&packet[1..], &[][..], &packet[1..], &packet[1..]] {
                NativePrefixScore::restore_silk(&incoming, &mut trial);
                let mut full = incoming.clone();
                let actual = scorer.decode(&mut trial, config, bytes);
                let expected = scorer.decode(&mut full, config, bytes);
                match (actual, expected) {
                    (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
                    (Err(actual), Err(expected)) => {
                        assert_eq!(actual.to_string(), expected.to_string());
                    }
                    results => panic!("snapshot result mismatch: {results:?}"),
                }
                // Compare future persistent state, not just this frame's PCM.
                for _ in 0..2 {
                    let mut a = vec![0; 960 * channels as usize];
                    let mut b = a.clone();
                    assert_eq!(
                        trial.decode_loss(&mut a, 960).unwrap(),
                        full.decode_loss(&mut b, 960).unwrap()
                    );
                    assert_eq!(a, b);
                    trial.decode(Some(packet), &mut a, false).unwrap();
                    full.decode(Some(packet), &mut b, false).unwrap();
                    assert_eq!(a, b);
                }
            }
        }
    }

    #[test]
    #[ignore = "explicit multi-layout production cache lifecycle packet/state proof"]
    fn practical_cache_lifecycle_packet_state_identity() {
        for rate in [crate::SampleRate::Hz24000, crate::SampleRate::Hz48000] {
            for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
                for milliseconds in [10, 20] {
                    let frame_size = rate as usize * milliseconds / 1000;
                    let mut optimized = crate::Encoder::new_auto(crate::EncoderConfig {
                        sample_rate: rate,
                        channels,
                        bitrate: 64000,
                        frame_size,
                    })
                    .unwrap();
                    let mut reference = optimized.clone();
                    let mut a_decoder = crate::Decoder::new(rate, channels).unwrap();
                    let mut b_decoder = a_decoder.clone();
                    for (frame, bitrate) in [32000, 64000, 128_000].into_iter().enumerate() {
                        let control = crate::EncoderControl {
                            bitrate,
                            bandwidth: crate::Bandwidth::Fullband,
                            mode: crate::EncoderMode::Hybrid,
                        };
                        optimized.set_control(control).unwrap();
                        reference.set_control(control).unwrap();
                        let input: Vec<_> = (0..frame_size)
                            .flat_map(|i| {
                                let x = (std::f32::consts::TAU
                                    * 173.0
                                    * (i + frame * frame_size) as f32
                                    / rate as u32 as f32)
                                    .sin()
                                    * 0.01;
                                [x, -x * 0.7].into_iter().take(channels as usize)
                            })
                            .collect();
                        let actual = optimized.encode(&input).unwrap();
                        super::REFERENCE_CACHE_LIFECYCLE.set(true);
                        let expected = reference.encode(&input).unwrap();
                        super::REFERENCE_CACHE_LIFECYCLE.set(false);
                        assert_eq!(
                            actual, expected,
                            "{rate:?} {channels:?} {milliseconds}ms frame={frame}"
                        );
                        let mut a = vec![0; frame_size * channels as usize];
                        let mut b = a.clone();
                        a_decoder.decode(Some(&actual), &mut a, false).unwrap();
                        b_decoder.decode(Some(&expected), &mut b, false).unwrap();
                        assert_eq!(a, b);
                    }
                    let actual = optimized.finish().unwrap();
                    super::REFERENCE_CACHE_LIFECYCLE.set(true);
                    let expected = reference.finish().unwrap();
                    super::REFERENCE_CACHE_LIFECYCLE.set(false);
                    assert_eq!(actual, expected);
                }
            }
        }
    }

    #[test]
    #[ignore = "reviewed music pool48 diagnostic; frozen layout/public matrices remain mandatory"]
    fn prototype_pool48_reviewed_music_screen() {
        POOL_MUSIC.set(true);
        prototype_pool48_first_screen();
        POOL_MUSIC.set(false);
    }

    std::thread_local! {
        static POOL_MUSIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[test]
    #[ignore = "test-only pool48 timing/quality screen; not production acceptance"]
    #[allow(clippy::similar_names)] // Decoder instance and decoded PCM are distinct test locals.
    fn prototype_pool48_first_screen() {
        super::POOL48.set(true);
        let music = if POOL_MUSIC.get() {
            let directory = std::env::var("MOOSICBOX_OPUS_QUALIFICATION_MEDIA").unwrap();
            let script = format!("{}/test_runner/src", env!("CARGO_MANIFEST_DIR"));
            let output = std::process::Command::new("python3").args(["-B", "-c", "import sys,wave;sys.path.insert(0,sys.argv[1]);from qualification_inputs import verify_inputs;_,p=verify_inputs(sys.argv[2]);w=wave.open(str(p['music']));w.setpos(w.getframerate()*2);sys.stdout.buffer.write(w.readframes(w.getframerate()))", &script, &directory]).output().unwrap();
            assert!(output.status.success());
            output
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32768.0)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for milliseconds in [10, 20] {
            for bitrate in [32000, 64000, 128_000] {
                let size = 48 * milliseconds;
                let config = crate::EncoderConfig {
                    sample_rate: crate::SampleRate::Hz48000,
                    channels: crate::Channels::Stereo,
                    bitrate,
                    frame_size: size,
                };
                let mut encoder = crate::Encoder::new_auto(config).unwrap();
                encoder
                    .set_control(crate::EncoderControl {
                        bitrate,
                        bandwidth: crate::Bandwidth::Fullband,
                        mode: crate::EncoderMode::Hybrid,
                    })
                    .unwrap();
                let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
                let mut source = Vec::new();
                let mut output = Vec::new();
                let started = std::time::Instant::now();
                let mut calls = 0;
                for frame in 0..20 {
                    let pcm: Vec<f32> = if music.is_empty() {
                        (0..size)
                            .flat_map(|i| {
                                let x = (std::f32::consts::TAU * 173.0 * (frame * size + i) as f32
                                    / 48000.0)
                                    .sin()
                                    * 0.01;
                                [x, -x * 0.7]
                            })
                            .collect()
                    } else {
                        music[frame * size * 2..(frame + 1) * size * 2].to_vec()
                    };
                    source.extend_from_slice(&pcm);
                    let packet = encoder.encode(&pcm).unwrap();
                    calls += super::POOL48_EVALUATIONS.get();
                    let mut decoded = vec![0; size * 2];
                    decoder.decode(Some(&packet), &mut decoded, false).unwrap();
                    output.extend(decoded);
                }
                let elapsed = started.elapsed();
                for packet in encoder.finish().unwrap() {
                    let mut decoded = vec![0; size * 2];
                    decoder.decode(Some(&packet), &mut decoded, false).unwrap();
                    output.extend(decoded);
                }
                let delay = encoder.lookahead() * 2;
                let signal: f64 = source.iter().map(|&x| f64::from(x).powi(2)).sum();
                let error: f64 = source
                    .iter()
                    .zip(&output[delay..])
                    .map(|(&x, &y)| (f64::from(x) - f64::from(y) / 32768.0).powi(2))
                    .sum();
                let snr = 10.0 * (signal / error).log10();
                eprintln!(
                    "POOL48_SCREEN bitrate={bitrate} ms={milliseconds} native_calls={calls} wall_ms={} realtime={} snr={snr}",
                    elapsed.as_millis(),
                    20.0 * milliseconds as f64 / elapsed.as_secs_f64() / 1000.0
                );
                assert!(
                    snr > 3.0,
                    "prototype quality failure {bitrate}/{milliseconds}: {snr}"
                );
            }
        }
        super::POOL48.set(false);
    }

    std::thread_local! {
        static TYPED_TIMING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[test]
    #[ignore = "typed semantics actual full bounded search on/off winner timing"]
    fn typed_full_search_timing() {
        TYPED_TIMING.set(true);
        channel_reuse_full_search_timing();
        TYPED_TIMING.set(false);
    }

    #[test]
    #[ignore = "exact full-search component cache on/off packet/state and 20-frame timing"]
    fn channel_reuse_full_search_timing() {
        use crate::silk::SilkDecoder;
        for bitrate in [32000, 64000, 128_000] {
            let config = crate::EncoderConfig {
                sample_rate: crate::SampleRate::Hz48000,
                channels: crate::Channels::Stereo,
                frame_size: 960,
                bitrate,
            };
            let mut optimized = crate::Encoder::new_auto(config).unwrap();
            optimized
                .set_control(crate::EncoderControl {
                    bitrate,
                    bandwidth: crate::Bandwidth::Fullband,
                    mode: crate::EncoderMode::Hybrid,
                })
                .unwrap();
            let mut baseline = optimized.clone();
            let mut a_decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
            let mut b_decoder = a_decoder.clone();
            let mut seed = 2457_u32;
            let mut off_ns = 0;
            let mut on_ns = 0;
            for frame in 0..20 {
                let input: Vec<_> = (0..1920)
                    .map(|_| {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        f32::from(i16::from_le_bytes(
                            seed.to_le_bytes()[..2].try_into().unwrap(),
                        )) / 131_072.0
                    })
                    .collect();
                let start = std::time::Instant::now();
                let expected = baseline.encode(&input).unwrap();
                off_ns += start.elapsed().as_nanos();
                let typed = crate::encoder::silk::TYPED_SCORE.replace(TYPED_TIMING.get());
                let old = SilkDecoder::channel_reuse_enabled(!TYPED_TIMING.get());
                let start = std::time::Instant::now();
                let actual = optimized.encode(&input).unwrap();
                on_ns += start.elapsed().as_nanos();
                crate::encoder::silk::TYPED_SCORE.set(typed);
                SilkDecoder::channel_reuse_enabled(old);
                assert_eq!(actual, expected, "packet {bitrate}/{frame}");
                let mut a = vec![0; 1920];
                let mut b = a.clone();
                a_decoder.decode(Some(&actual), &mut a, false).unwrap();
                b_decoder.decode(Some(&expected), &mut b, false).unwrap();
                assert_eq!(a, b);
            }
            let expected = baseline.finish().unwrap();
            let old = SilkDecoder::channel_reuse_enabled(true);
            let actual = optimized.finish().unwrap();
            SilkDecoder::channel_reuse_enabled(old);
            assert_eq!(actual, expected);
            eprintln!(
                "CHANNEL_REUSE_FULL bitrate={bitrate} frames=20 off_ns={off_ns} on_ns={on_ns} realtime={}",
                400_000_000.0 / on_ns as f64
            );
        }
    }

    #[test]
    #[ignore = "canonical decoded-channel typed continuation proof, no analysis shortcuts"]
    fn canonical_typed_channel_boundary_proof() {
        for size in [160, 320] {
            for scale in [0.001, 0.2, 0.99] {
                let (packets, semantics) =
                    crate::encoder::silk::component_semantic_fixture(size, scale);
                for rate in [crate::SampleRate::Hz24000, crate::SampleRate::Hz48000] {
                    for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
                        crate::silk::SilkDecoder::prove_typed_boundary(
                            &packets, size, rate, channels, &semantics,
                        );
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "smallest exact component reuse proof: four fixed-origin mid/side combinations"]
    fn channel_component_2x2_proof() {
        use crate::silk::SilkDecoder;
        for size in [160, 320] {
            for rate in [crate::SampleRate::Hz24000, crate::SampleRate::Hz48000] {
                for channels in [crate::Channels::Mono, crate::Channels::Stereo] {
                    for scale in [0.001_f32, 0.2, 0.99] {
                        let packets = crate::encoder::silk::component_reuse_fixture(size, scale);
                        let config = crate::EncoderConfig {
                            sample_rate: crate::SampleRate::Hz16000,
                            channels: crate::Channels::Stereo,
                            bitrate: 64000,
                            frame_size: size,
                        };
                        let count = rate as usize * size / 16000;
                        let scorer = NativePrefixScore {
                            incoming: crate::Decoder::new(rate, channels).unwrap(),
                            target: vec![0; count * channels as usize],
                            cache: NativeScoreCache::default(),
                            best: None,
                        };
                        let mut incoming = scorer.incoming.clone();
                        incoming
                            .configure_silk(
                                crate::SampleRate::Hz16000,
                                config.channels,
                                if size == 160 { 10 } else { 20 },
                            )
                            .unwrap();
                        // Cold, then a nonzero committed origin. Each batch gets
                        // a fresh holder; never mix origins in the cache.
                        for warm in [false, true] {
                            if warm {
                                scorer.decode(&mut incoming, config, &packets[0]).unwrap();
                            }
                            let mut trial = incoming.clone();
                            for packet in &packets {
                                let old = SilkDecoder::channel_reuse_enabled(true);
                                NativePrefixScore::restore_silk(&incoming, &mut trial);
                                let actual = scorer.decode(&mut trial, config, packet).unwrap();
                                SilkDecoder::channel_reuse_enabled(old);
                                let mut full = incoming.clone();
                                let expected = scorer.decode(&mut full, config, packet).unwrap();
                                assert_eq!(
                                    actual, expected,
                                    "range/PCM {size}/{rate:?}/{channels:?}/{scale}/{warm}"
                                );
                                // Mutate clones so the holder remains batch-local;
                                // compare PLC, FEC fallback, recovery and future PCM.
                                let mut a = trial.clone();
                                a.silk.detach_channel_reuse();
                                let mut b = full.clone();
                                let mut x = vec![0; count * channels as usize];
                                let mut y = x.clone();
                                a.decode_loss(&mut x, count).unwrap();
                                b.decode_loss(&mut y, count).unwrap();
                                assert_eq!(x, y);
                                let mut wire = vec![if size == 160 { 0x44 } else { 0x4c }];
                                wire.extend_from_slice(&packets[1]);
                                assert_eq!(
                                    a.decode(Some(&wire), &mut x, true).unwrap(),
                                    b.decode(Some(&wire), &mut y, true).unwrap()
                                );
                                assert_eq!(x, y);
                                assert_eq!(
                                    scorer.decode(&mut a, config, &packets[3]).unwrap(),
                                    scorer.decode(&mut b, config, &packets[3]).unwrap()
                                );
                            }
                            let stats = trial.silk.channel_reuse_counts();
                            eprintln!(
                                "COMPONENT_2X2 size={size} rate={rate:?} channels={channels:?} scale={scale} warm={warm} calls_hits_syntheses_retained={stats:?}"
                            );
                            assert!(stats.1 > 0, "must exercise channel reuse");
                            assert!(stats.3 <= 128);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn zero_suffix_key_is_lossless_for_arbitrary_bytes() {
        let mut identities = std::collections::BTreeMap::new();
        for length in 0..=8 {
            for bits in 0..1_usize << length {
                let input: Vec<u8> = (0..length)
                    .map(|i| if bits & (1 << i) == 0 { 0 } else { 255 })
                    .collect();
                let prefix = replay_key_prefix(&input);
                let key = (input.len(), prefix.to_vec());
                let mut restored = prefix.to_vec();
                restored.resize(input.len(), 0);
                assert_eq!(restored, input);
                if let Some(previous) = identities.insert(key, input.clone()) {
                    assert_eq!(previous, input);
                }
            }
        }
        // Nonzero tail bytes (including raw entropy) must never be omitted.
        assert_eq!(replay_key_prefix(&[0, 0, 1, 0, 128]), &[0, 0, 1, 0, 128]);
    }
}
