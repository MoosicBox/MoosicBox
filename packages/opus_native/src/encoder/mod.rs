//! Native Opus audio encoding.
//!
//! The initial encoder targets constant-rate, long-window CELT packets. Analysis,
//! energy quantization, algebraic pulse quantization and entropy coding run in Rust;
//! no platform codec is used. Explicit SILK and hybrid paths use integer LPC
//! and excitation coding. Voiced SILK pitch/LTP optimization is not implemented.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
// Codec dimensions bound these conversions. Floating-point input is converted
// once to Q12; all subsequent analysis and quantization uses integer arithmetic.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

mod allocation;
mod analysis;
#[cfg(feature = "hybrid")]
mod hybrid;
mod quantize;
mod resample;
#[cfg(feature = "silk")]
mod silk;

use crate::celt::constants::{CELT_BINS_2_5MS, ENERGY_PROB_MODEL};
use crate::range::{RangeEncoder, RangeEncoderError};
use crate::{Channels, Error, Result, SampleRate};

/// Input framing and bitrate configuration for a native Opus encoder.
#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    /// Input PCM sample rate: 8, 12, 16, 24 or 48 kHz.
    pub sample_rate: SampleRate,
    /// Number of interleaved input channels.
    pub channels: Channels,
    /// Target maximum packet bitrate, including the Opus TOC byte, in bits/s.
    /// CELT uses constant-sized packets; SILK packets omit unused payload bytes.
    pub bitrate: u32,
    /// Input samples per channel: 2.5, 5, 10 or 20 ms at the input rate.
    pub frame_size: usize,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            sample_rate: SampleRate::Hz48000,
            channels: Channels::Stereo,
            bitrate: 128_000,
            frame_size: 960,
        }
    }
}

/// Stateful, pure-Rust PCM-to-Opus encoder.
///
/// [`Self::new`] selects CELT. Feature-gated constructors select SILK or hybrid.
/// Mode selection is fixed for the lifetime of the encoder.
///
/// Supply exactly one frame of normalized interleaved PCM to [`Self::encode`].
/// The caller must trim the initial [`Self::lookahead`] samples and the padded
/// final packet using container granule positions, rather than dropping packets.
#[derive(Clone)]
pub struct Encoder {
    config: EncoderConfig,
    #[cfg(feature = "silk")]
    silk: Option<silk::SilkEncoder>,
    #[cfg(feature = "hybrid")]
    hybrid: Option<hybrid::LowBand>,
    analysis: analysis::Analysis,
    interpolator: resample::Interpolator,
    finished: bool,
    started: bool,
}

fn entropy(error: RangeEncoderError) -> Error {
    Error::Unsupported(format!("CELT encoder entropy coding: {error}"))
}

impl Encoder {
    /// Construct a constant-rate CELT encoder.
    ///
    /// # Errors
    /// * Returns `Unsupported` for a frame size other than 2.5/5/10/20 ms,
    ///   or bitrate above 256,000 bits/s or below the duration minimum:
    ///   128,000 (2.5 ms), 64,000 (5 ms), 32,000 (10/20 ms).
    pub fn new(config: EncoderConfig) -> Result<Self> {
        let factor = rate_factor(config.sample_rate);
        if !matches!(
            config.frame_size.saturating_mul(factor),
            120 | 240 | 480 | 960
        ) || !(32_000..=256_000).contains(&config.bitrate)
        {
            return Err(Error::Unsupported(
                "native CELT encoder requires 2.5/5/10/20 ms and 32..=256 kbit/s".into(),
            ));
        }
        let minimum = (15_360_000 / (config.frame_size * factor)).max(32_000) as u32;
        if config.bitrate < minimum {
            return Err(Error::Unsupported(format!(
                "CELT frame duration requires at least {minimum} bit/s"
            )));
        }
        Ok(Self {
            config,
            #[cfg(feature = "silk")]
            silk: None,
            #[cfg(feature = "hybrid")]
            hybrid: None,
            analysis: analysis::Analysis::new(channel_count(config.channels)),
            interpolator: resample::Interpolator::new(channel_count(config.channels), factor),
            finished: false,
            started: false,
        })
    }

    /// Construct an explicit unvoiced-prediction mono/stereo SILK encoder.
    ///
    /// # Errors
    /// * Returns `Unsupported` unless input is 8/12/16 kHz, 10/20 ms,
    ///   and bitrate is 48,000..=128,000 bits/s mono or 96,000..=128,000 stereo.
    #[cfg(feature = "silk")]
    pub fn new_silk(config: EncoderConfig) -> Result<Self> {
        let minimum = if config.channels == Channels::Stereo {
            96_000
        } else {
            48_000
        };
        if !(minimum..=128_000).contains(&config.bitrate) {
            return Err(Error::Unsupported(format!(
                "standalone SILK encoder requires {minimum}..=128000 bit/s for this channel layout"
            )));
        }
        let silk = silk::SilkEncoder::new(config)?;
        let mut encoder = Self::new(EncoderConfig::default())?;
        encoder.config = config;
        encoder.silk = Some(silk);
        Ok(encoder)
    }

    /// Construct a fullband hybrid encoder with shared SILK/CELT entropy state.
    ///
    /// # Errors
    /// * Returns `Unsupported` unless input is 48 kHz, 10/20 ms and the target
    ///   bitrate is 128,000..=256,000 bits/s.
    #[cfg(feature = "hybrid")]
    pub fn new_hybrid(config: EncoderConfig) -> Result<Self> {
        if config.sample_rate != SampleRate::Hz48000
            || !matches!(config.frame_size, 480 | 960)
            || !(128_000..=256_000).contains(&config.bitrate)
        {
            return Err(Error::Unsupported(
                "hybrid encoder requires 48 kHz, 10/20 ms and 128..=256 kbit/s".into(),
            ));
        }
        let mut encoder = Self::new(config)?;
        encoder.silk = Some(silk::SilkEncoder::new(EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            frame_size: config.frame_size / 3,
            bitrate: if config.channels == Channels::Stereo {
                (config.bitrate * 3 / 4).min(128_000)
            } else {
                config.bitrate / 2
            },
            ..config
        })?);
        encoder.hybrid = Some(hybrid::LowBand::new(channel_count(config.channels))?);
        Ok(encoder)
    }

    /// Encoder configuration.
    #[must_use]
    pub const fn config(&self) -> EncoderConfig {
        self.config
    }

    /// Algorithmic delay in input samples per channel.
    ///
    /// CELT has 120 internal samples of overlap plus input interpolation delay.
    /// SILK includes its same-rate decoder output delay; hybrid aligns both
    /// branches to 384 samples at 48 kHz. No automatic mode switching is used.
    #[must_use]
    pub const fn lookahead(&self) -> usize {
        #[cfg(feature = "hybrid")]
        if self.hybrid.is_some() {
            return hybrid::DELAY;
        }
        #[cfg(feature = "silk")]
        if self.silk.is_some() {
            return match self.config.sample_rate {
                SampleRate::Hz8000 => 5,
                SampleRate::Hz12000 => 10,
                _ => 13,
            };
        }
        let factor = rate_factor(self.config.sample_rate);
        120 / factor + if factor > 1 { 12 } else { 0 }
    }

    /// Encode one complete frame of normalized, interleaved PCM.
    ///
    /// # Errors
    /// * Returns `Unsupported` for a finished encoder, wrong input length,
    ///   non-finite PCM, or PCM outside -1..=1.
    /// * Returns an error if entropy coding exceeds the configured packet budget.
    ///   State is unchanged on error.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        if self.finished
            || pcm.len() != self.config.frame_size * channel_count(self.config.channels)
            || pcm.iter().any(|x| !x.is_finite() || x.abs() > 1.0)
        {
            return Err(Error::Unsupported(
                "invalid encoder PCM frame or encoder already finished".into(),
            ));
        }
        let mut next = self.clone();
        let packet = next.encode_frame(pcm)?;
        next.started = true;
        *self = next;
        Ok(packet)
    }

    /// Flush the analysis overlap, returning one padded packet if audio was supplied.
    /// Repeated calls return an empty vector. Empty streams produce no packets.
    ///
    /// Container writers must retain the original input sample count when
    /// computing the final granule position: flush padding is not input audio.
    ///
    /// # Errors
    /// * Returns an error if the final packet exceeds its entropy budget; state
    ///   is unchanged and flushing can be retried.
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>> {
        if self.finished {
            return Ok(Vec::new());
        }
        let mut next = self.clone();
        let mut packets = Vec::new();
        if self.started {
            for _ in 0..self.lookahead().div_ceil(self.config.frame_size) {
                packets.push(next.encode_frame(&vec![
                    0.0;
                    self.config.frame_size
                        * channel_count(self.config.channels)
                ])?);
            }
        }
        next.finished = true;
        *self = next;
        Ok(packets)
    }

    #[allow(clippy::too_many_lines)] // Ordered CELT packet entropy stages.
    fn encode_frame(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        #[cfg(feature = "hybrid")]
        let hybrid = self.hybrid.is_some();
        #[cfg(not(feature = "hybrid"))]
        let hybrid = false;
        #[cfg(feature = "silk")]
        if !hybrid && let Some(silk) = &mut self.silk {
            return silk.encode(pcm);
        }
        let start = if hybrid { 17 } else { 0 };
        let channels = channel_count(self.config.channels);
        #[cfg(feature = "hybrid")]
        let delayed = self.hybrid.as_mut().map(|low| low.delay_celt(pcm));
        #[cfg(feature = "hybrid")]
        let analysis_pcm = delayed.as_deref().unwrap_or(pcm);
        #[cfg(not(feature = "hybrid"))]
        let analysis_pcm = pcm;
        let resampled = self.interpolator.process(analysis_pcm);
        let spectrum = self.analysis.transform(&resampled);
        let internal_size = self.config.frame_size * rate_factor(self.config.sample_rate);
        let lm = (internal_size / 120).ilog2() as usize;
        let (target_energy, mut energy) = quantize::energies(&spectrum, lm);
        let bytes = (self.config.bitrate as usize * internal_size / 384_000).saturating_sub(1);
        let mut enc = RangeEncoder::new(bytes).map_err(entropy)?;
        #[cfg(feature = "hybrid")]
        if let Some(lowband) = &mut self.hybrid {
            let low = lowband.process(pcm);
            self.silk
                .as_mut()
                .ok_or_else(|| Error::Unsupported("missing hybrid SILK state".into()))?
                .encode_prefix(&low, &mut enc)?;
            if enc.tell() as usize + 37 <= bytes * 8 {
                enc.ec_enc_bit_logp(false, 12).map_err(entropy)?;
            }
        }
        if !hybrid {
            enc.ec_enc_bit_logp(false, 15).map_err(entropy)?; // Not the silence shortcut.
            enc.ec_enc_bit_logp(false, 1).map_err(entropy)?; // No pitch postfilter.
        }
        if lm > 0 {
            enc.ec_enc_bit_logp(false, 3).map_err(entropy)?;
        } // Long MDCT.
        enc.ec_enc_bit_logp(true, 3).map_err(entropy)?; // Intra energy prediction.
        let model = &ENERGY_PROB_MODEL[lm][1];
        let mut prediction = vec![0_i32; channels];
        for band in start..21 {
            for (channel, prediction) in prediction.iter_mut().enumerate() {
                let index = channel * 21 + band;
                let mut qi = (target_energy[index] - *prediction + 32768) >> 16;
                let remaining = bytes * 8 - enc.tell() as usize;
                if remaining >= 15 {
                    quantize::laplace(
                        &mut enc,
                        &mut qi,
                        u32::from(model[2 * band]) << 7,
                        u32::from(model[2 * band + 1]) << 6,
                    )?;
                } else if remaining >= 2 {
                    qi = qi.clamp(-1, 1);
                    enc.ec_enc_icdf(((2 * qi) ^ -i32::from(qi < 0)) as usize, &[2, 1, 0], 2)
                        .map_err(entropy)?;
                } else if remaining >= 1 {
                    qi = qi.clamp(-1, 0);
                    enc.ec_enc_bit_logp(qi != 0, 1).map_err(entropy)?;
                } else {
                    qi = -1;
                }
                energy[index] = (*prediction + qi * 65536).clamp(-28 * 65536, 28 * 65536);
                *prediction += qi * (65536 - 9830);
            }
        }
        // No time/frequency resolution changes: LM=3, non-transient, select=0
        // maps every band to resolution zero.
        let mut budget = bytes * 8;
        let reserved = lm > 0 && enc.tell() as usize + 5 <= budget;
        budget -= usize::from(reserved);
        for band in start..21 {
            let logp = if band == start { 4 } else { 5 };
            if enc.tell() as usize + logp as usize <= budget {
                enc.ec_enc_bit_logp(false, logp).map_err(entropy)?;
            }
        }
        // Non-transient TF select is not signalled when all changes are zero.
        if enc.tell() as usize + 4 <= bytes * 8 {
            enc.ec_enc_icdf(0, &[25, 23, 2, 0], 5).map_err(entropy)?; // No spreading.
        }
        for _ in start..21 {
            if enc.tell_frac() as usize + 48 < bytes * 64 {
                enc.ec_enc_bit_logp(false, 6).map_err(entropy)?;
            }
        }
        if enc.tell_frac() as usize + 48 <= bytes * 64 {
            enc.ec_enc_icdf_u16(5, &[126, 124, 119, 109, 87, 41, 19, 9, 4, 2, 0], 7)
                .map_err(entropy)?;
        }
        let remaining = bytes as i32 * 64 - enc.tell_frac() as i32 - 1;
        let allocation = allocation::allocate(&mut enc, remaining, channels, lm as i32, start)?;
        quantize::fine_energy(
            &mut enc,
            &target_energy,
            &mut energy,
            &allocation.fine,
            channels,
        )?;
        let mut balance = allocation.balance;
        let mut offset = 0;
        for (band, width) in CELT_BINS_2_5MS.iter().enumerate() {
            let n = usize::from(*width) << lm;
            if band < start {
                offset += n;
                continue;
            }
            let tell = enc.tell_frac() as i32;
            if band != start {
                balance -= tell;
            }
            let mut remaining = bytes as i32 * 64 - tell - 1;
            let bits = if band < allocation.coded {
                (allocation.bits[band] + balance / (3.min(allocation.coded - band) as i32))
                    .min(remaining + 1)
                    .clamp(0, 16383)
            } else {
                0
            };
            for values in &spectrum {
                quantize::partition(
                    &mut enc,
                    &values[offset..offset + n],
                    bits / channels as i32,
                    band,
                    lm as i32,
                    &mut remaining,
                )?;
            }
            balance += allocation.bits[band] + tell;
            offset += n;
        }
        quantize::final_energy(
            &mut enc,
            &target_energy,
            &mut energy,
            &allocation.fine,
            &allocation.priority,
            channels,
            bytes * 8,
            start,
        )?;
        let payload = enc.finish().map_err(entropy)?;
        let mut packet = Vec::with_capacity(bytes + 1);
        packet.push(
            (if hybrid {
                0x70 | ((u8::from(lm == 3)) << 3)
            } else {
                0xe0 | ((lm as u8) << 3)
            }) | if channels == 2 { 4 } else { 0 },
        );
        packet.extend(payload);
        Ok(packet)
    }
}

const fn channel_count(channels: Channels) -> usize {
    match channels {
        Channels::Mono => 1,
        Channels::Stereo => 2,
    }
}

const fn rate_factor(rate: SampleRate) -> usize {
    match rate {
        SampleRate::Hz8000 => 6,
        SampleRate::Hz12000 => 4,
        SampleRate::Hz16000 => 3,
        SampleRate::Hz24000 => 2,
        SampleRate::Hz48000 => 1,
    }
}
