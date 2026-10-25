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
    let length = replay
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |last| last + 1);
    &replay[..length]
}

#[derive(Clone)]
#[cfg_attr(not(test), derive(Default))]
pub(super) struct NativeScoreCache {
    scores: ReplayScores,
    #[cfg(test)]
    pub calls: usize,
    #[cfg(test)]
    pub hits: usize,
    #[cfg(test)]
    pub disabled: bool,
}

#[cfg(test)]
impl Default for NativeScoreCache {
    fn default() -> Self {
        Self {
            scores: ReplayScores::new(),
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

#[derive(Clone)]
pub(super) struct NativePrefixScore {
    pub incoming: crate::Decoder,
    pub target: Vec<i16>,
    pub cache: NativeScoreCache,
    pub best: Option<(i128, Vec<u8>, super::EncoderConfig)>,
}

impl NativePrefixScore {
    pub(super) fn decode(
        &self,
        decoder: &mut crate::Decoder,
        config: super::EncoderConfig,
        bytes: &[u8],
    ) -> Result<(Vec<i16>, u32)> {
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
        let mut pcm = vec![0; self.target.len()];
        let samples = decoder.decode_silk_payload(&mut range, layout, config.channels, &mut pcm)?;
        assert_eq!(samples * super::channel_count(config.channels), pcm.len());
        Ok((pcm, range.ec_tell()))
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
    fn score(&mut self, config: super::EncoderConfig, replay: &[u8]) -> Result<i128> {
        let key = (
            config.sample_rate as u32,
            super::channel_count(config.channels),
            config.frame_size,
            replay.len(),
        );
        let stored = replay_key_prefix(replay);
        #[cfg(test)]
        {
            self.cache.calls += 1;
        }
        let cached = self
            .cache
            .scores
            .get(&key)
            .and_then(|scores| scores.get(stored))
            .copied();
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
            let mut decoder = self.incoming.clone();
            let (pcm, _) = self.decode(&mut decoder, config, replay)?;
            let error = pcm
                .iter()
                .zip(&self.target)
                .map(|(&a, &b)| i128::from(i32::from(a) - i32::from(b)).pow(2))
                .sum();
            if enabled {
                self.cache
                    .scores
                    .entry(key)
                    .or_default()
                    .insert(stored.to_vec(), error);
            }
            error
        };
        if self.best.as_ref().is_none_or(|old| error < old.0) {
            self.best = Some((error, replay.to_vec(), config));
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
    use super::replay_key_prefix;

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
