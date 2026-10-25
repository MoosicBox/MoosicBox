//! Integer hybrid analysis with decoder-output phase compensation.
//! The SILK interpolation filter is dispersive near the band edge. A finite
//! inverse-response analysis filter aligns its output with CELT, rather than
//! treating frequency-dependent phase as a constant sample delay.
use crate::{Channels, Result};

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
