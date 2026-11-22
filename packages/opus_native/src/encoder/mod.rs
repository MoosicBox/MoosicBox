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

mod adaptive;
mod allocation;
pub use adaptive::{EncoderControl, EncoderMode};
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
/// [`Self::new_auto`] enables frame-boundary runtime policy without changing input framing.
///
/// Supply exactly one frame of normalized interleaved PCM to [`Self::encode`].
/// The caller must trim the initial [`Self::lookahead`] samples and the padded
/// final packet using container granule positions, rather than dropping packets.
#[derive(Clone)]
pub struct Encoder {
    config: EncoderConfig,
    adaptive: Option<adaptive::Adaptive>,
    #[cfg(feature = "hybrid")]
    adaptive_low: Vec<f32>,
    #[cfg(feature = "silk")]
    silk: Option<silk::SilkEncoder>,
    #[cfg(feature = "silk")]
    silk_input: Option<resample::SilkInput>,
    #[cfg(feature = "silk")]
    silk_input_delay: usize,
    #[cfg(feature = "hybrid")]
    hybrid: Option<hybrid::LowBand>,
    #[cfg(feature = "hybrid")]
    hybrid_input: Option<resample::Interpolator>,
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
    ///   64,000 mono / 128,000 stereo (2.5 ms), 32,000 mono / 64,000 stereo
    ///   (5 ms), 32,000 (10/20 ms).
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
        let minimum = if config.channels == Channels::Mono {
            (7_680_000 / (config.frame_size * factor)).max(32_000) as u32
        } else {
            (15_360_000 / (config.frame_size * factor)).max(32_000) as u32
        };
        if config.bitrate < minimum {
            return Err(Error::Unsupported(format!(
                "CELT frame duration requires at least {minimum} bit/s"
            )));
        }
        Ok(Self {
            config,
            adaptive: None,
            #[cfg(feature = "hybrid")]
            adaptive_low: Vec::new(),
            #[cfg(feature = "silk")]
            silk: None,
            #[cfg(feature = "silk")]
            silk_input: None,
            #[cfg(feature = "silk")]
            silk_input_delay: 0,
            #[cfg(feature = "hybrid")]
            hybrid: None,
            #[cfg(feature = "hybrid")]
            hybrid_input: None,
            analysis: analysis::Analysis::new(channel_count(config.channels)),
            interpolator: resample::Interpolator::new(channel_count(config.channels), factor),
            finished: false,
            started: false,
        })
    }

    /// Construct an explicit unvoiced-prediction mono/stereo SILK encoder.
    ///
    /// # Errors
    /// * Returns `Unsupported` for invalid durations (resampled 24/48 kHz input
    ///   requires 10/20/40/60 ms), or bitrate outside 48,000..=128,000 bits/s mono
    ///   or 96,000..=128,000 stereo.
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
        let converted = matches!(
            config.sample_rate,
            SampleRate::Hz24000 | SampleRate::Hz48000
        );
        let internal = if converted {
            let rate = config.sample_rate as usize;
            if ![rate / 100, rate / 50, rate / 25, rate * 3 / 50].contains(&config.frame_size) {
                return Err(adaptive::unavailable(
                    "resampled SILK input requires 10/20/40/60 ms",
                ));
            }
            EncoderConfig {
                sample_rate: SampleRate::Hz16000,
                frame_size: config.frame_size * 16000 / rate,
                ..config
            }
        } else {
            config
        };
        let silk = silk::SilkEncoder::new(internal)?;
        let mut encoder = Self::new(EncoderConfig::default())?;
        encoder.config = config;
        encoder.silk = Some(silk);
        if converted {
            let input = resample::SilkInput::new(
                channel_count(config.channels),
                config.sample_rate as u32,
            )?;
            // Characterize actual decoder conversion, including SILK's one-sample
            // output delay. Dispersive phase is not a scaled same-rate delay.
            let mut output =
                crate::silk::resampler::SilkResampler::new(16000, config.sample_rate as u32)?;
            let mut impulse = vec![0_i16; 160];
            impulse[1] = 16384;
            let response = output.process(&impulse)?;
            let peak = response
                .iter()
                .enumerate()
                .max_by_key(|(_, value)| value.unsigned_abs())
                .map_or(0, |(index, _)| index);
            encoder.silk_input_delay = input.lookahead() + peak;
            encoder.silk_input = Some(input);
        }
        Ok(encoder)
    }

    /// Construct a fullband hybrid encoder with shared SILK/CELT entropy state.
    ///
    /// # Errors
    /// * Returns `Unsupported` unless input is 24/48 kHz, 10/20 ms and the target
    ///   bitrate is 32,000..=256,000 bits/s for 48 kHz stereo, or
    ///   128,000..=256,000 bits/s for other supported input layouts.
    #[cfg(feature = "hybrid")]
    pub fn new_hybrid(config: EncoderConfig) -> Result<Self> {
        let factor = rate_factor(config.sample_rate);
        let minimum =
            if config.sample_rate == SampleRate::Hz48000 && config.channels == Channels::Stereo {
                32_000
            } else {
                128_000
            };
        if !matches!(
            config.sample_rate,
            SampleRate::Hz24000 | SampleRate::Hz48000
        ) || !matches!(config.frame_size.saturating_mul(factor), 480 | 960)
            || !(minimum..=256_000).contains(&config.bitrate)
        {
            return Err(Error::Unsupported(format!(
                "hybrid encoder requires 24/48 kHz, 10/20 ms and {minimum}..=256000 bit/s"
            )));
        }
        let mut encoder = Self::new(config)?;
        encoder.silk = Some(silk::SilkEncoder::new(EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            frame_size: config.frame_size * factor / 3,
            bitrate: hybrid::packet_lowband_bitrate(
                config.bitrate,
                config.channels,
                config.frame_size * factor,
            ),
            ..config
        })?);
        encoder.hybrid = Some(hybrid::LowBand::new(channel_count(config.channels))?);
        if factor > 1 {
            encoder.hybrid_input = Some(resample::Interpolator::new(
                channel_count(config.channels),
                factor,
            ));
            encoder.interpolator = resample::Interpolator::new(channel_count(config.channels), 1);
        }
        Ok(encoder)
    }

    /// Construct an adaptive encoder without changing the default CELT constructor.
    ///
    /// Controls are applied between frames, preserving analysis and overlap state.
    ///
    /// # Errors
    /// * Rejects inputs other than 48 kHz and 10/20 ms.
    /// * Propagates the currently validated CELT bitrate limits.
    pub fn new_auto(config: EncoderConfig) -> Result<Self> {
        #[cfg(not(feature = "hybrid"))]
        {
            let _ = config;
            Err(adaptive::unavailable(
                "adaptive encoding requires the hybrid feature",
            ))
        }
        #[cfg(feature = "hybrid")]
        Self::new_auto_hybrid(config)
    }

    #[cfg(feature = "hybrid")]
    fn new_auto_hybrid(config: EncoderConfig) -> Result<Self> {
        if config.sample_rate != SampleRate::Hz48000 || !matches!(config.frame_size, 480 | 960) {
            return Err(adaptive::unavailable(
                "adaptive encoding requires 48 kHz, 10/20 ms",
            ));
        }
        let mut encoder = Self::new(config)?;
        encoder.adaptive = Some(adaptive::Adaptive::new(config.bitrate));
        #[cfg(feature = "hybrid")]
        {
            encoder.hybrid = Some(hybrid::LowBand::new(channel_count(config.channels))?);
            encoder.silk = Some(silk::SilkEncoder::new(EncoderConfig {
                sample_rate: SampleRate::Hz16000,
                frame_size: config.frame_size / 3,
                bitrate: hybrid::packet_lowband_bitrate(
                    config.bitrate,
                    config.channels,
                    config.frame_size,
                ),
                ..config
            })?);
        }
        Ok(encoder)
    }

    /// Apply controls at the next frame boundary, without consuming input.
    /// Invalid controls leave configuration and predictive state unchanged.
    ///
    /// # Errors
    /// * Rejects controls after finishing or on explicit predictive constructors.
    /// * Rejects unqualified bandwidth, bitrate and mode transitions.
    pub fn set_control(&mut self, control: EncoderControl) -> Result<()> {
        if self.finished {
            return Err(adaptive::unavailable("cannot control a finished encoder"));
        }
        if self.adaptive.is_none()
            && matches!(self.active_mode(), EncoderMode::Silk | EncoderMode::Hybrid)
        {
            return Err(adaptive::unavailable(
                "explicit predictive controls are not yet qualified",
            ));
        }
        let internal_size = self.config.frame_size * rate_factor(self.config.sample_rate);
        let minimum = if self.config.channels == Channels::Mono {
            (7_680_000 / internal_size).max(32_000) as u32
        } else {
            (15_360_000 / internal_size).max(32_000) as u32
        };
        adaptive::validate_celt(control, minimum)?;
        #[cfg(not(feature = "hybrid"))]
        if control.mode != EncoderMode::Celt {
            return Err(adaptive::unavailable(
                "adaptive transitions require the hybrid feature",
            ));
        }
        if self.adaptive.is_none() && control.mode != EncoderMode::Celt {
            return Err(adaptive::unavailable(
                "use new_auto to enable adaptive mode",
            ));
        }
        let steady_hybrid = self.steady_hybrid_control(control);
        let hybrid_minimum = if self.config.frame_size == 480 {
            192_000
        } else {
            160_000
        };
        let silk_target = control.mode == EncoderMode::Silk
            || (control.mode == EncoderMode::Auto
                && control.bandwidth == crate::Bandwidth::Wideband);
        if silk_target {
            let minimum = if self.config.channels == Channels::Stereo {
                96_000
            } else {
                48_000
            };
            if !(minimum..=128_000).contains(&control.bitrate) {
                return Err(adaptive::unavailable(
                    "SILK controls retain the validated standalone bitrate limits",
                ));
            }
            if self.active_mode() == EncoderMode::Celt && control.bitrate < 128_000 {
                return Err(adaptive::unavailable(
                    "CELT to SILK transition requires 128 kbit/s redundancy budget; lower bitrate after entry",
                ));
            }
        }
        if self.adaptive.is_some()
            && (control.mode == EncoderMode::Hybrid || self.active_mode() == EncoderMode::Hybrid)
            && control.bitrate < hybrid_minimum
            && !silk_target
            && !steady_hybrid
        {
            return Err(adaptive::unavailable(
                "hybrid transition requires a reserved redundancy budget; switch to CELT before lowering bitrate",
            ));
        }
        if self.config.frame_size == 480
            && control.mode == EncoderMode::Auto
            && control.bandwidth == crate::Bandwidth::Fullband
            && control.bitrate < 192_000
            && control.bitrate >= 160_000
        {
            return Err(adaptive::unavailable(
                "10 ms adaptive hybrid requires at least 192 kbit/s",
            ));
        }
        self.config.bitrate = control.bitrate;
        if let Some(state) = &mut self.adaptive {
            state.control = control;
        }
        Ok(())
    }

    /// Whether a practical-rate control keeps the current hybrid stream, with
    /// neither a mode boundary nor a pending redundant CELT departure.
    fn steady_hybrid_control(&self, control: EncoderControl) -> bool {
        self.config.sample_rate == SampleRate::Hz48000
            && self.config.channels == Channels::Stereo
            && matches!(self.config.frame_size, 480 | 960)
            && (32_000..=128_000).contains(&control.bitrate)
            && control.bandwidth == crate::Bandwidth::Fullband
            && matches!(control.mode, EncoderMode::Auto | EncoderMode::Hybrid)
            && self
                .adaptive
                .as_ref()
                .is_some_and(|state| state.active == EncoderMode::Hybrid && !state.next_celt)
    }

    /// Requested runtime controls. Explicit constructors report their fixed mode.
    #[must_use]
    pub fn control(&self) -> EncoderControl {
        self.adaptive.as_ref().map_or_else(
            || EncoderControl {
                bitrate: self.config.bitrate,
                bandwidth: match (self.active_mode(), self.config.sample_rate) {
                    (EncoderMode::Silk, SampleRate::Hz8000) => crate::Bandwidth::Narrowband,
                    (EncoderMode::Silk, SampleRate::Hz12000) => crate::Bandwidth::Mediumband,
                    (EncoderMode::Silk, _) => crate::Bandwidth::Wideband,
                    _ => crate::Bandwidth::Fullband,
                },
                mode: self.active_mode(),
            },
            |state| state.control,
        )
    }

    /// Actual coding mode of the most recently encoded frame (or initial mode).
    #[must_use]
    pub const fn active_mode(&self) -> EncoderMode {
        if let Some(state) = &self.adaptive {
            return state.active;
        }
        #[cfg(feature = "hybrid")]
        if self.hybrid.is_some() {
            return EncoderMode::Hybrid;
        }
        #[cfg(feature = "silk")]
        if self.silk.is_some() {
            return EncoderMode::Silk;
        }
        EncoderMode::Celt
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
    /// branches to 384 samples at 48 kHz. Auto retains this delay across modes.
    #[must_use]
    pub const fn lookahead(&self) -> usize {
        if self.adaptive.is_some() {
            return 384;
        }
        #[cfg(feature = "hybrid")]
        if self.hybrid.is_some() {
            let factor = rate_factor(self.config.sample_rate);
            return hybrid::DELAY / factor + if factor > 1 { 12 } else { 0 };
        }
        #[cfg(feature = "silk")]
        if self.silk_input.is_some() {
            return self.silk_input_delay;
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
        if self.adaptive.is_some() {
            return self.encode_adaptive_frame(pcm);
        }
        #[cfg(feature = "silk")]
        if let Some(input) = &mut self.silk_input {
            let converted = input.process(pcm)?;
            return self
                .silk
                .as_mut()
                .ok_or_else(|| adaptive::unavailable("missing resampled SILK state"))?
                .encode(&converted);
        }
        #[cfg(feature = "hybrid")]
        if let Some(input) = &mut self.hybrid_input {
            // Convert once before splitting the low/high bands. Keep the public
            // input clock unchanged; internal framing exists only during coding.
            let converted: Vec<f32> = input
                .process(pcm)
                .into_iter()
                .map(|sample| (f64::from(sample) / 134_217_728.0) as f32)
                .collect();
            let config = self.config;
            self.config.sample_rate = SampleRate::Hz48000;
            self.config.frame_size *= rate_factor(config.sample_rate);
            let packet = self.encode_coding_frame(&converted, None);
            self.config = config;
            return packet;
        }
        self.encode_coding_frame(pcm, None)
    }

    /// Experimental standard non-redundant boundary, inaccessible to public controls.
    /// The decoder conceals/fades the old mode for its first 5 ms, resets the new
    /// CELT state and (on CELT entry to hybrid) resets SILK/resampler state.
    #[cfg(all(test, feature = "hybrid"))]
    fn encode_unredundant_candidate(
        &mut self,
        pcm: &[f32],
        control: EncoderControl,
    ) -> Result<Vec<u8>> {
        assert!(!self.finished, "candidate cannot encode after finish");
        assert_eq!(
            pcm.len(),
            self.config.frame_size * channel_count(self.config.channels)
        );
        // This test-only entry bypasses encode() but must preserve its lifecycle
        // bookkeeping so finish() emits the unchanged lookahead flush.
        self.started = true;
        let previous = self.active_mode();
        let target = control.mode;
        assert!(matches!(target, EncoderMode::Celt | EncoderMode::Hybrid));
        self.config.bitrate = control.bitrate;
        let state = self.adaptive.as_mut().expect("adaptive test encoder");
        state.control = control;
        state.next_celt = false;
        state.end = 21;
        let low = self.hybrid.as_mut().expect("hybrid test filters");
        self.adaptive_low = low.process(pcm);
        let delayed = low.delay_celt(pcm);
        if previous != target {
            if target == EncoderMode::Hybrid {
                self.silk.as_mut().expect("test SILK state").reset();
            }
            // Non-redundant boundaries reset decoder CELT overlap to zero.
            // Do not seed a full-band MDCT overlap which the decoder cannot
            // reconstruct: its separately concealed old-mode head supplies
            // the transition fade. Keep the real-tail prefill in the existing
            // redundant production path, whose decoder does receive that tail.
            let channels = channel_count(self.config.channels);
            self.analysis = analysis::Analysis::new(channels);
            self.interpolator = resample::Interpolator::new(channels, 1);
        }
        self.adaptive.as_mut().unwrap().active = target;
        self.silk
            .as_mut()
            .unwrap()
            .set_bitrate(hybrid::packet_lowband_bitrate(
                control.bitrate,
                self.config.channels,
                self.config.frame_size,
            ))?;
        let packet = self.encode_coding_frame(&delayed, None)?;
        let channels = channel_count(self.config.channels);
        self.adaptive.as_mut().unwrap().tail = delayed[delayed.len() - 120 * channels..].to_vec();
        Ok(packet)
    }

    #[cfg(feature = "hybrid")]
    fn encode_adaptive_frame(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        let (mut active, target, leaving) = {
            let state = self
                .adaptive
                .as_ref()
                .ok_or_else(|| adaptive::unavailable("missing adaptive state"))?;
            (state.active, state.target(), state.next_celt)
        };
        if leaving {
            active = EncoderMode::Celt;
            let state = self.adaptive.as_mut().unwrap();
            state.next_celt = false;
            state.active = active;
        }
        let entering = active == EncoderMode::Celt && target != EncoderMode::Celt;
        let departing = active != EncoderMode::Celt && target == EncoderMode::Celt;
        let packet_mode = if departing { active } else { target };
        self.adaptive.as_mut().unwrap().end = if packet_mode == EncoderMode::Silk {
            17
        } else {
            21
        };
        // Both analysis branches advance once, regardless of the selected packet mode.
        // Their fixed delays are identical to explicit hybrid (384 samples).
        let low = self
            .hybrid
            .as_mut()
            .ok_or_else(|| adaptive::unavailable("missing adaptive filters"))?;
        self.adaptive_low = low.process(pcm);
        let delayed = low.delay_celt(pcm);
        self.silk
            .as_mut()
            .ok_or_else(|| adaptive::unavailable("missing adaptive SILK"))?
            .set_bitrate(if packet_mode == EncoderMode::Silk {
                self.config.bitrate.min(128_000)
            } else {
                hybrid::packet_lowband_bitrate(
                    self.config.bitrate,
                    self.config.channels,
                    self.config.frame_size * rate_factor(self.config.sample_rate),
                )
            })?;
        let saved_analysis = self.analysis.clone();
        let saved_interpolator = self.interpolator.clone();
        let redundancy =
            if entering {
                // The old CELT overlap must reach the redundant head before high-band
                // reset. It is not a new encoder and does not consume another frame.
                Some(self.encode_redundancy(
                    &delayed[..240 * channel_count(self.config.channels)],
                    false,
                )?)
            } else if departing {
                Some(self.encode_redundancy(&delayed, true)?)
            } else {
                None
            };
        if departing {
            self.analysis = saved_analysis;
            self.interpolator = saved_interpolator;
        }
        if entering {
            self.silk.as_mut().unwrap().reset();
            // Decoder CELT state is reset at this boundary as prescribed by Opus.
            // Prefill from real delayed PCM, not silence or a reconstructed encoder.
            self.prefill_celt();
            self.adaptive.as_mut().unwrap().active = packet_mode;
        } else if !departing && active != target {
            if target == EncoderMode::Hybrid {
                self.prefill_celt();
            }
            self.adaptive.as_mut().unwrap().active = target;
        }
        let packet = if packet_mode == EncoderMode::Silk {
            self.encode_adaptive_silk(redundancy.as_deref().map(|data| (entering, data)))?
        } else {
            self.encode_coding_frame(&delayed, redundancy.as_deref().map(|data| (entering, data)))?
        };
        if departing {
            // The redundant tail primes the following CELT packet. Publish the
            // change only on the next frame: this packet still carries hybrid TOC.
            self.adaptive.as_mut().unwrap().next_celt = true;
            self.prime_redundant_tail(&delayed)?;
        }
        let channels = channel_count(self.config.channels);
        self.adaptive.as_mut().unwrap().tail = delayed[delayed.len() - 120 * channels..].to_vec();
        Ok(packet)
    }

    #[cfg(feature = "hybrid")]
    fn encode_adaptive_silk(&mut self, redundant: Option<(bool, &[u8])>) -> Result<Vec<u8>> {
        let silk = self
            .silk
            .as_mut()
            .ok_or_else(|| adaptive::unavailable("missing adaptive SILK"))?;
        if redundant.is_none() {
            return silk.encode(&self.adaptive_low);
        }
        let (to_silk, data) = redundant.unwrap();
        let maximum = self.config.bitrate as usize * self.config.frame_size / 384_000 - 1;
        let reserved_bitrate = (data.len() * 384_000).div_ceil(self.config.frame_size) as u32;
        silk.set_bitrate(
            self.config
                .bitrate
                .saturating_sub(reserved_bitrate)
                .min(128_000),
        )?;
        let mut probe = RangeEncoder::new(maximum.saturating_sub(data.len())).map_err(entropy)?;
        let mut trial = silk.clone();
        trial.encode_prefix(&self.adaptive_low, &mut probe)?;
        probe.ec_enc_bit_logp(to_silk, 1).map_err(entropy)?;
        let bytes = (probe.tell() as usize).div_ceil(8);
        if bytes + data.len() > maximum {
            return Err(adaptive::unavailable(
                "SILK redundancy exceeds packet budget",
            ));
        }
        // SILK infers redundancy length from the shared coder's tell, so the
        // entropy prefix must have exactly that many bytes (never truncate it).
        let mut enc = RangeEncoder::new(bytes).map_err(entropy)?;
        silk.encode_prefix(&self.adaptive_low, &mut enc)?;
        enc.ec_enc_bit_logp(to_silk, 1).map_err(entropy)?;
        let mut packet = vec![
            (8 + u8::from(self.config.frame_size == 960)) << 3
                | if self.config.channels == Channels::Stereo {
                    4
                } else {
                    0
                },
        ];
        packet.extend(enc.finish().map_err(entropy)?);
        packet.extend_from_slice(data);
        Ok(packet)
    }

    #[cfg(feature = "hybrid")]
    fn prefill_celt(&mut self) {
        let channels = channel_count(self.config.channels);
        self.analysis = analysis::Analysis::new(channels);
        self.interpolator = resample::Interpolator::new(channels, 1);
        if let Some(state) = &self.adaptive
            && !state.tail.is_empty()
        {
            let input = self.interpolator.process(&state.tail);
            self.analysis.transform(&input);
        }
    }

    #[cfg(feature = "hybrid")]
    fn prime_redundant_tail(&mut self, pcm: &[f32]) -> Result<()> {
        let _ = self.encode_redundancy(pcm, true)?;
        Ok(())
    }

    #[cfg(feature = "hybrid")]
    fn encode_redundancy(&mut self, pcm: &[f32], tail: bool) -> Result<Vec<u8>> {
        let saved_config = self.config;
        let saved_active = self.adaptive.as_ref().unwrap().active;
        let channels = channel_count(self.config.channels);
        if tail {
            self.analysis = analysis::Analysis::new(channels);
            self.interpolator = resample::Interpolator::new(channels, 1);
            let prefill = &pcm[pcm.len() - 360 * channels..pcm.len() - 240 * channels];
            let input = self.interpolator.process(prefill);
            self.analysis.transform(&input);
        }
        self.config.frame_size = 240;
        self.config.bitrate = 128_000;
        self.adaptive.as_mut().unwrap().active = EncoderMode::Celt;
        let input = if tail {
            &pcm[pcm.len() - 240 * channels..]
        } else {
            pcm
        };
        let result = self.encode_coding_frame(input, None);
        self.config = saved_config;
        self.adaptive.as_mut().unwrap().active = saved_active;
        result.map(|packet| packet[1..].to_vec())
    }

    #[allow(clippy::too_many_lines)]
    fn encode_coding_frame(
        &mut self,
        pcm: &[f32],
        redundant: Option<(bool, &[u8])>,
    ) -> Result<Vec<u8>> {
        #[cfg(feature = "hybrid")]
        let hybrid = self.hybrid.is_some()
            && self
                .adaptive
                .as_ref()
                .is_none_or(|s| s.active == EncoderMode::Hybrid);
        #[cfg(not(feature = "hybrid"))]
        let hybrid = false;
        #[cfg(feature = "silk")]
        if self.adaptive.is_none()
            && !hybrid
            && let Some(silk) = &mut self.silk
        {
            return silk.encode(pcm);
        }
        let end = self.adaptive.as_ref().map_or(21, |state| state.end);
        let start = if hybrid { 17 } else { 0 };
        let channels = channel_count(self.config.channels);
        #[cfg(feature = "hybrid")]
        let delayed = self
            .hybrid
            .as_mut()
            .filter(|_| self.adaptive.is_none())
            .map(|low| low.delay_celt(pcm));
        #[cfg(feature = "hybrid")]
        let analysis_pcm = delayed.as_deref().unwrap_or(pcm);
        #[cfg(not(feature = "hybrid"))]
        let analysis_pcm = pcm;
        let resampled = self.interpolator.process(analysis_pcm);
        let internal_size = self.config.frame_size * rate_factor(self.config.sample_rate);
        let bytes = (self.config.bitrate as usize * internal_size / 384_000)
            .saturating_sub(1 + redundant.map_or(0, |(_, data)| data.len()));
        let (spectrum, transient) = self
            .analysis
            .transform_celt(&resampled, !hybrid && bytes >= 24 * channels);
        let lm = (internal_size / 120).ilog2() as usize;
        let (target_energy, mut energy) = quantize::energies(&spectrum, lm);
        let mut enc = RangeEncoder::new(bytes).map_err(entropy)?;
        #[cfg(feature = "hybrid")]
        if hybrid && let Some(lowband) = &mut self.hybrid {
            let low = if self.adaptive.is_some() {
                self.adaptive_low.clone()
            } else {
                lowband.process(pcm)
            };
            hybrid::encode_lowband_prefix(
                self.silk
                    .as_mut()
                    .ok_or_else(|| Error::Unsupported("missing hybrid SILK state".into()))?,
                &low,
                &mut enc,
                self.config.bitrate,
            )?;
            if enc.tell() as usize + 37 <= bytes * 8 {
                enc.ec_enc_bit_logp(redundant.is_some(), 12)
                    .map_err(entropy)?;
                if let Some((to_silk, data)) = redundant {
                    enc.ec_enc_bit_logp(to_silk, 1).map_err(entropy)?;
                    enc.ec_enc_uint((data.len() - 2) as u32, 256)
                        .map_err(entropy)?;
                }
            } else if redundant.is_some() {
                return Err(adaptive::unavailable(
                    "hybrid transition cannot fit redundancy signaling",
                ));
            }
        }
        if !hybrid {
            enc.ec_enc_bit_logp(false, 15).map_err(entropy)?; // Not the silence shortcut.
            enc.ec_enc_bit_logp(false, 1).map_err(entropy)?; // No pitch postfilter.
        }
        if lm > 0 {
            enc.ec_enc_bit_logp(transient, 3).map_err(entropy)?;
        } // MDCT block layout.
        enc.ec_enc_bit_logp(true, 3).map_err(entropy)?; // Intra energy prediction.
        let model = &ENERGY_PROB_MODEL[lm][1];
        let mut prediction = vec![0_i32; channels];
        for band in start..end {
            for (channel, prediction) in prediction.iter_mut().enumerate() {
                let index = channel * 21 + band;
                let mut qi = (target_energy[index] - *prediction + 32768) >> 16;
                let remaining = (bytes * 8).saturating_sub(enc.tell() as usize);
                qi = quantize::coarse_energy_budget(qi, remaining, channels, band, start, end);
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
        if end == 21 {
            quantize::tf_neutral(&mut enc, start, lm, transient, bytes * 8)?;
        } else {
            quantize::tf_neutral_end(&mut enc, start, end, lm, transient, bytes * 8)?;
        }
        if enc.tell() as usize + 4 <= bytes * 8 {
            enc.ec_enc_icdf(0, &[25, 23, 2, 0], 5).map_err(entropy)?; // No spreading.
        }
        for _ in start..end {
            if enc.tell_frac() as usize + 48 < bytes * 64 {
                enc.ec_enc_bit_logp(false, 6).map_err(entropy)?;
            }
        }
        if enc.tell_frac() as usize + 48 <= bytes * 64 {
            enc.ec_enc_icdf_u16(5, &[126, 124, 119, 109, 87, 41, 19, 9, 4, 2, 0], 7)
                .map_err(entropy)?;
        }
        let remaining = bytes as i32 * 64 - enc.tell_frac() as i32 - 1;
        let anti_collapse = transient && lm >= 2 && remaining >= ((lm as i32 + 2) << 3);
        let remaining = remaining - 8 * i32::from(anti_collapse);
        let intensity = if channels == 2 && self.config.bitrate <= 40_000 {
            13
        } else if channels == 2 && self.config.bitrate <= 64_000 {
            17
        } else {
            21
        };
        let intensity = allocation::stereo_intensity(&spectrum, lm, intensity);
        let allocation = if hybrid {
            allocation::allocate(&mut enc, remaining, channels, lm as i32, start)?
        } else if channels == 2 {
            allocation::allocate_joint(
                &mut enc,
                remaining,
                channels,
                lm as i32,
                start,
                end,
                intensity.min(end),
            )?
        } else {
            allocation::allocate_celt_end(
                &mut enc,
                remaining,
                channels,
                lm as i32,
                start,
                end,
                intensity.min(end),
            )?
        };
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
            if band >= end {
                break;
            }
            if band < start {
                offset += n;
                continue;
            }
            let tell = enc.tell_frac() as i32;
            if band != start {
                balance -= tell;
            }
            let mut remaining = bytes as i32 * 64 - tell - 1 - 8 * i32::from(anti_collapse);
            let bits = if band < allocation.coded {
                (allocation.bits[band] + balance / (3.min(allocation.coded - band) as i32))
                    .min(remaining + 1)
                    .clamp(0, 16383)
            } else {
                0
            };
            let blocks = if transient { 1 << lm } else { 1 };
            if channels == 2 && band >= allocation.intensity {
                quantize::intensity_band(
                    &mut enc,
                    &spectrum[0][offset..offset + n],
                    &spectrum[1][offset..offset + n],
                    bits,
                    band,
                    lm as i32,
                    &mut remaining,
                    blocks,
                )?;
            } else if channels == 2 && !allocation.dual {
                quantize::stereo_band(
                    &mut enc,
                    &spectrum[0][offset..offset + n],
                    &spectrum[1][offset..offset + n],
                    bits,
                    band,
                    lm as i32,
                    &mut remaining,
                    blocks,
                )?;
            } else {
                for values in &spectrum {
                    quantize::band(
                        &mut enc,
                        &values[offset..offset + n],
                        bits / channels as i32,
                        band,
                        lm as i32,
                        &mut remaining,
                        blocks,
                    )?;
                }
            }
            balance += allocation.bits[band] + tell;
            offset += n;
        }
        if anti_collapse {
            enc.ec_enc_bits(0, 1).map_err(entropy)?;
        }
        if end == 21 {
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
        } else {
            quantize::final_energy_end(
                &mut enc,
                &target_energy,
                &mut energy,
                &allocation.fine,
                &allocation.priority,
                channels,
                bytes * 8,
                start,
                end,
            )?;
        }
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
        if let Some((_, data)) = redundant {
            packet.extend_from_slice(data);
        }
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
