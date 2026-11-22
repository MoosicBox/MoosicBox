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

#[cfg(test)]
std::thread_local! {
    static HYBRID_TEST_ENERGY_DB: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
    static RD_UNFACTORED_REFERENCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static RD_VERIFY_CANDIDATES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
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
    #[cfg(feature = "hybrid")]
    practical: Option<Box<hybrid::PracticalSynthesis>>,
    #[cfg(feature = "hybrid")]
    hybrid_energy_db: i32,
    #[cfg(feature = "hybrid")]
    hybrid_tilt_db: i32,
    #[cfg(feature = "hybrid")]
    proposal_amp: f32,
    #[cfg(feature = "hybrid")]
    quick_prefix: bool,
    #[cfg(feature = "hybrid")]
    practical_prefix_bitrate: Option<u32>,
    #[cfg(feature = "hybrid")]
    native_prefix_score: Option<Box<hybrid::NativePrefixScore>>,
    analysis: analysis::Analysis,
    interpolator: resample::Interpolator,
    finished: bool,
    started: bool,
}

#[cfg(feature = "hybrid")]
struct PracticalCandidate {
    #[cfg(test)]
    bias: i32,
    error: i128,
    encoder: Encoder,
    synthesis: crate::Decoder,
    packet: Vec<u8>,
}

#[derive(Clone)]
struct PreparedCeltFrame {
    joint_hybrid: bool,
    range: RangeEncoder,
    spectrum: Vec<Vec<i32>>,
    target_energy: Vec<i32>,
    energy: Vec<i32>,
    transient: bool,
    hybrid: bool,
    channels: usize,
    lm: usize,
    bytes: usize,
    bitrate: u32,
    start: usize,
    end: usize,
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
            #[cfg(feature = "hybrid")]
            practical: None,
            #[cfg(feature = "hybrid")]
            hybrid_energy_db: 0,
            #[cfg(feature = "hybrid")]
            hybrid_tilt_db: 0,
            #[cfg(feature = "hybrid")]
            proposal_amp: 1.,
            #[cfg(feature = "hybrid")]
            quick_prefix: false,
            #[cfg(feature = "hybrid")]
            practical_prefix_bitrate: None,
            #[cfg(feature = "hybrid")]
            native_prefix_score: None,
            analysis: analysis::Analysis::new(channel_count(config.channels)),
            interpolator: resample::Interpolator::new(channel_count(config.channels), factor),
            finished: false,
            started: false,
        })
    }

    /// Construct an explicit mono/stereo SILK encoder.
    ///
    /// # Errors
    /// * Returns `Unsupported` for durations other than 10/20/40/60 ms.
    /// * Returns `Unsupported` for bitrate outside 16,000..=128,000 bits/s.
    ///   Low-rate native/converted layouts are undergoing public qualification.
    #[cfg(feature = "silk")]
    pub fn new_silk(config: EncoderConfig) -> Result<Self> {
        let minimum = 16_000;
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
        let minimum = 32_000;
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
        // Practical rates share the transactional native-scored cold-entry path.
        // Higher rates retain their established packet construction.
        if config.bitrate <= 128_000 {
            let mut encoder = Self::new_auto_hybrid(config)?;
            encoder.set_control(EncoderControl {
                mode: EncoderMode::Hybrid,
                bitrate: config.bitrate,
                bandwidth: crate::Bandwidth::Fullband,
            })?;
            encoder
                .adaptive
                .as_mut()
                .ok_or_else(|| adaptive::unavailable("missing adaptive Hybrid state"))?
                .fixed_controls = true;
            return Ok(encoder);
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
        if !matches!(
            config.sample_rate,
            SampleRate::Hz24000 | SampleRate::Hz48000
        ) || !matches!(
            config.frame_size * rate_factor(config.sample_rate),
            480 | 960
        ) {
            return Err(adaptive::unavailable(
                "adaptive encoding requires 24/48 kHz, 10/20 ms",
            ));
        }
        Self::new_auto_layout(config, false)
    }

    // Qualifying is retained for historical test callers; every admitted layout
    // uses the same production synthesis history.
    #[cfg(feature = "hybrid")]
    fn new_auto_layout(config: EncoderConfig, _qualifying: bool) -> Result<Self> {
        let factor = rate_factor(config.sample_rate);
        if !matches!(
            config.sample_rate,
            SampleRate::Hz24000 | SampleRate::Hz48000
        ) || !matches!(config.frame_size * factor, 480 | 960)
        {
            return Err(adaptive::unavailable(
                "practical layout requires 24/48 kHz, 10/20 ms",
            ));
        }
        let mut encoder = Self::new(config)?;
        encoder.adaptive = Some(adaptive::Adaptive::new(config.bitrate));
        {
            // Start synthesis with the first packet, including CELT warm-up,
            // so later practical controls never enter with a cold shadow.
            encoder.practical = Some(Box::new(hybrid::PracticalSynthesis::with_layout(
                config.sample_rate,
                config.channels,
                hybrid::DELAY / factor + if factor > 1 { 12 } else { 0 },
            )?));
        }
        #[cfg(feature = "hybrid")]
        {
            encoder.hybrid = Some(hybrid::LowBand::new(channel_count(config.channels))?);
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
            if factor > 1 {
                encoder.hybrid_input = Some(resample::Interpolator::new(
                    channel_count(config.channels),
                    factor,
                ));
                encoder.interpolator =
                    resample::Interpolator::new(channel_count(config.channels), 1);
            }
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
        if self.adaptive.as_ref().map_or_else(
            || matches!(self.active_mode(), EncoderMode::Silk | EncoderMode::Hybrid),
            |adaptive| adaptive.fixed_controls,
        ) {
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
        #[cfg(feature = "hybrid")]
        if self.practical.is_some()
            && hybrid::coding_layout(control.bandwidth, self.config.channels, internal_size).is_ok()
            && matches!(
                control.bandwidth,
                crate::Bandwidth::SuperWideband | crate::Bandwidth::Fullband
            )
            && (32_000..=128_000).contains(&control.bitrate)
            && matches!(
                control.mode,
                EncoderMode::Auto | EncoderMode::Hybrid | EncoderMode::Celt
            )
        {
            self.config.bitrate = control.bitrate;
            let state = self.adaptive.as_mut().ok_or_else(|| {
                adaptive::unavailable("practical control requires adaptive state")
            })?;
            state.control = control;
            return Ok(());
        }
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
        matches!(
            self.config.sample_rate,
            SampleRate::Hz24000 | SampleRate::Hz48000
        ) && matches!(
            self.config.frame_size * rate_factor(self.config.sample_rate),
            480 | 960
        ) && (32_000..=128_000).contains(&control.bitrate)
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
            let factor = rate_factor(self.config.sample_rate);
            return 384 / factor + if factor > 1 { 12 } else { 0 };
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
        #[cfg(feature = "hybrid")]
        // At the native CELT clock, analyse the same signed PCM grid as practical synthesis.
        // Lower-rate inputs retain their precision through interpolation.
        let cleaned;
        #[cfg(feature = "hybrid")]
        let pcm = if self.practical.is_some()
            && self.control().mode == EncoderMode::Celt
            && self.config.sample_rate == SampleRate::Hz48000
        {
            cleaned = pcm
                .iter()
                .map(|&v| (v * 32768.0).round().clamp(-32768.0, 32767.0) / 32768.0)
                .collect::<Vec<_>>();
            &cleaned[..]
        } else {
            pcm
        };
        let packet = next.encode_frame(pcm, false)?;
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
                packets.push(next.encode_frame(
                    &vec![0.0; self.config.frame_size * channel_count(self.config.channels)],
                    true,
                )?);
            }
        }
        next.finished = true;
        *self = next;
        Ok(packets)
    }

    #[allow(clippy::too_many_lines)] // Ordered CELT packet entropy stages.
    fn encode_frame(
        &mut self,
        pcm: &[f32],
        #[cfg_attr(not(feature = "hybrid"), allow(unused_variables))] flush: bool,
    ) -> Result<Vec<u8>> {
        #[cfg(feature = "hybrid")]
        if self.practical.is_some() {
            return self.encode_practical_frame(pcm, flush);
        }
        self.encode_frame_inner(pcm)
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_lines)] // Bounded transactional RD loop commits packet, predictive state and synthesis together.
    fn encode_practical_frame(&mut self, pcm: &[f32], flush: bool) -> Result<Vec<u8>> {
        #[cfg(test)]
        if hybrid::POOL48.get() {
            hybrid::POOL48_EVALUATIONS.set(0);
        }
        // Remove the boxed synthesis snapshot before cloning encoder candidates.
        // The public encode/finish transaction owns rollback if all trials fail.
        let mut shadow = self
            .practical
            .take()
            .ok_or_else(|| adaptive::unavailable("missing practical synthesis"))?;
        let target = shadow.target(pcm);
        let control = self
            .adaptive
            .as_ref()
            .ok_or_else(|| adaptive::unavailable("practical synthesis requires adaptive state"))?
            .control;
        let internal_size = self.config.frame_size * rate_factor(self.config.sample_rate);
        let eligible = matches!(
            self.config.sample_rate,
            SampleRate::Hz24000 | SampleRate::Hz48000
        ) && matches!(internal_size, 480 | 960)
            && matches!(
                control.bandwidth,
                crate::Bandwidth::SuperWideband | crate::Bandwidth::Fullband
            )
            && (32_000..=128_000).contains(&control.bitrate)
            && matches!(control.mode, EncoderMode::Auto | EncoderMode::Hybrid);
        let departing = self.active_mode() == EncoderMode::Hybrid
            && control.mode == EncoderMode::Celt
            && control.bitrate <= 64_000;
        let nominal =
            hybrid::packet_lowband_bitrate(control.bitrate, self.config.channels, internal_size);
        // Share the delayed target boundary across all suffixes. Only the zero-input
        // SILK proposal needs an extra prefix; it uses the nominal packet split.
        let split = if eligible
            && internal_size == 480
            && (64_000..128_000).contains(&control.bitrate)
            && matches!(
                (self.config.sample_rate, self.config.channels),
                (SampleRate::Hz48000, Channels::Mono) | (SampleRate::Hz24000, Channels::Stereo)
            ) {
            hybrid::release_start(&target)
        } else {
            target.len()
        };
        let release = split < target.len();
        // When the buffered input falls to digital silence, compare both higher and
        // lower CELT energy before the delayed tail reaches the packet boundary.
        let ending = eligible
            && internal_size == 480
            && pcm[pcm.len() / 2..].iter().all(|&v| v == 0.)
            && pcm[..pcm.len() / 2]
                .iter()
                .any(|v| v.abs() > 32.0 / 32768.0);
        let budgets = [
            nominal,
            nominal * 3 / 4,
            nominal / 2,
            nominal * 9 / 8,
            nominal * 5 / 4,
        ];
        // At low stereo rates the nominal split reserves bytes for CELT. Also
        // try spending more on SILK when that improves the decoded full frame.
        // Keep nominal first for ties and enforce the same total packet budget.
        let budget_count = if !eligible {
            1
        } else if self.config.channels == Channels::Stereo
            && internal_size == 480
            && control.bitrate < 64_000
        {
            budgets.len()
        } else {
            3
        };
        let mut last_error = None;
        #[cfg(test)]
        let mut phases = [0_u128; 4];
        let mut best: Option<PracticalCandidate> = None;
        // One immutable synthesis origin/target for this input frame. Budget
        // changes affect parameter selection, never prefix decoding semantics.
        let mut prefix_origin = shadow.decoder.clone();
        if self.active_mode() == EncoderMode::Celt {
            prefix_origin.silk.reset_decoder_state();
            prefix_origin.silk_resamplers.clear();
            prefix_origin.silk_resampler_input_rate = 0;
        }
        let mut prefix_cache = hybrid::NativeScoreCache::default();
        #[cfg(not(test))]
        let share_scores = true;
        #[cfg(test)]
        let share_scores = !hybrid::REFERENCE_CACHE_LIFECYCLE.get();
        let near = eligible
            && internal_size == 480
            && control.bitrate < 64000
            && self.config.sample_rate == SampleRate::Hz48000
            && self.config.channels == Channels::Stereo
            && target.iter().any(|v| v.unsigned_abs() > 2)
            && target.iter().all(|v| v.unsigned_abs() < 128);
        let quiet_fast = eligible
            && internal_size == 480
            && control.bitrate < 64000
            && self.config.sample_rate == SampleRate::Hz48000
            && self.config.channels == Channels::Stereo
            && target.iter().all(|x| x.unsigned_abs() < 128);
        // Try the nominal allocation first. Stop expanding the search when the actual
        // decoded packet has zero error or consumes less than a third of signal power in error.
        let energy: i128 = target.iter().map(|&v| i128::from(v).pow(2)).sum();
        let shortlist = eligible
            && internal_size == 480
            && control.bitrate == 32_000
            && self.config.channels == Channels::Stereo
            && target.iter().any(|v| v.unsigned_abs() >= 128)
            // At 24 kHz the resampling delay can put an attack or its release
            // on opposite sides of the source and reconstruction windows. Use
            // the quick split only for sustained activity across both windows.
            && (self.config.sample_rate == SampleRate::Hz48000
                || (target.chunks_exact(target.len() / 4).all(|group| {
                    group.iter().any(|v| v.unsigned_abs() >= 128)
                }) && pcm.chunks_exact(pcm.len() / 4).all(|group| {
                    group.iter().any(|v| v.abs() >= 128.0 / 32768.0)
                })));
        // On a padded final packet there is no following predictor state to
        // preserve. Compare the rate-controlled splits against the delayed tail.
        let edge = eligible
            && internal_size == 480
            && control.bitrate == 32_000
            && self.config.channels == Channels::Stereo
            && self.config.sample_rate == SampleRate::Hz24000
            && (flush
                || (target.iter().all(|v| v.unsigned_abs() < 128)
                    && pcm.iter().any(|v| v.abs() >= 1.0 / 32.0)));
        for &quick in if edge {
            &[true][..]
        } else if quiet_fast || shortlist {
            &[true, false][..]
        } else {
            &[false][..]
        } {
            for (amp, budget) in (if release {
                &[1., 0.][..]
            } else if near && !quick {
                &[1., 0.66, 1.33][..]
            } else {
                &[1.][..]
            })
            .iter()
            .flat_map(|&a| budgets[..budget_count].iter().map(move |&b| (a, b)))
            {
                if release && amp == 0.0 && budget != nominal {
                    continue;
                }
                if best.as_ref().is_some_and(|b| {
                    (quiet_fast && (b.error == 0 || b.error * 3 < energy))
                        || ((shortlist || edge) && b.error * 2 < energy)
                }) {
                    break;
                }
                if quick && budget != nominal && !edge {
                    continue;
                }
                let mut prepared_state = None;
                let mut prepared_frame = None;
                if eligible {
                    let mut state = self.clone();
                    state.hybrid_energy_db = 0;
                    state.quick_prefix = quick;
                    state.proposal_amp = amp;
                    state.practical_prefix_bitrate = Some(budget);
                    {
                        state.native_prefix_score = Some(Box::new(hybrid::NativePrefixScore {
                            incoming: prefix_origin.clone(),
                            target: target.clone(),
                            cache: std::mem::take(&mut prefix_cache),
                            best: None,
                        }));
                    }
                    // A smaller allocation can be unrepresentable (e.g. SILK's
                    // minimum excitation exceeds its budget). Discard only this
                    // cloned candidate, as for failed suffix candidates below.
                    #[cfg(test)]
                    let phase = std::time::Instant::now();
                    #[cfg(test)]
                    if RD_UNFACTORED_REFERENCE.get() {
                        state.native_prefix_score.as_mut().unwrap().cache.disabled = true;
                    }
                    let prefix_result = state.encode_practical_boundary_prepared(
                        pcm,
                        EncoderControl {
                            mode: EncoderMode::Hybrid,
                            ..control
                        },
                        Some(&mut prepared_frame),
                    );
                    #[cfg(test)]
                    {
                        phases[0] += phase.elapsed().as_nanos();
                    }
                    // Move completed scores out before the five suffix clones.
                    // No scorer callback occurs in a captured CELT suffix. Keep
                    // incoming/target/best for independent prefix verification.
                    if share_scores && let Some(scorer) = &mut state.native_prefix_score {
                        prefix_cache = std::mem::take(&mut scorer.cache);
                    }
                    if let Err(error) = prefix_result {
                        last_error = Some(error);
                        continue;
                    }
                    if budget == nominal
                    && eligible
                    // The low-band gain proposals exclude this near-floor regime.
                    && internal_size == 480
                    && self.config.sample_rate == SampleRate::Hz48000
                    && control.bitrate < 64000
                    && target.len() == 960
                    && target.iter().all(|v| v.unsigned_abs() <= 2)
                    && target.iter().any(|&v| v != 0)
                    && let Some(f) = prepared_frame.as_ref()
                    && f.end >= 20
                    {
                        let ch = f.channels;
                        let n = target.len() / ch;
                        let mut analysis = analysis::Analysis::new(ch);
                        let mut prev = vec![0; target.len()];
                        for i in 0..120 * ch {
                            prev[(n - 120) * ch + i] = i32::from(target[i]) * 4096;
                        }
                        analysis.transform(&prev);
                        let mut next = vec![0; target.len()];
                        for i in 0..(n - 120) * ch {
                            next[i] = i32::from(target[120 * ch + i]) * 4096;
                        }
                        let spec = analysis.transform(&next);
                        let (energy, _) = quantize::energies(&spec, f.lm);
                        let left: i64 =
                            target.iter().step_by(2).map(|&v| i64::from(v).pow(2)).sum();
                        let right: i64 = target
                            .iter()
                            .skip(1)
                            .step_by(2)
                            .map(|&v| i64::from(v).pow(2))
                            .sum();
                        let total = left + right;
                        let peak = target
                            .iter()
                            .map(|x| i64::from(x.unsigned_abs()))
                            .max()
                            .unwrap();
                        let crest =
                            (target.len() as i64 * peak * peak / total).max(1).ilog2() as i32;
                        for db in [crest, crest + 3, crest + 6] {
                            for tilt in [-3, 0, 3] {
                                for balance in [0, if right <= left { -6 } else { 6 }] {
                                    let mut alt = f.clone();
                                    alt.spectrum.clone_from(&spec);
                                    alt.target_energy.clone_from(&energy);
                                    for c in 0..ch {
                                        for b in f.start..f.end {
                                            let ix = c * 21 + b;
                                            if b < f.start + 3 {
                                                alt.target_energy[ix] += (db
                                                    + tilt * (b as i32 - f.start as i32 - 1)
                                                    + balance * c as i32)
                                                    * 10885;
                                            } else {
                                                alt.target_energy[ix] = -28 * 65536;
                                            }
                                        }
                                    }
                                    let Ok(packet) = Self::encode_celt_suffix(alt, None) else {
                                        continue;
                                    };
                                    let mut dec = shadow.decoder.clone();
                                    let mut y = vec![0; target.len()];
                                    if dec.decode(Some(&packet), &mut y, false).ok()
                                        != Some(self.config.frame_size)
                                    {
                                        continue;
                                    }
                                    let e = y
                                        .iter()
                                        .zip(&target)
                                        .map(|(&a, &b)| (i128::from(a) - i128::from(b)).pow(2))
                                        .sum();
                                    if best.as_ref().is_none_or(|old| e < old.error) {
                                        best = Some(PracticalCandidate {
                                            #[cfg(test)]
                                            bias: db,
                                            error: e,
                                            encoder: state.clone(),
                                            synthesis: dec,
                                            packet,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    // The unfactored control still needs the alternate near-floor
                    // envelopes above, but must rebuild the prefix for ordinary trials.
                    #[cfg(test)]
                    if RD_UNFACTORED_REFERENCE.get() {
                        prepared_frame = None;
                    }
                    if prepared_frame.is_some() {
                        prepared_state = Some(state);
                    }
                }
                let quiet = self.config.sample_rate == SampleRate::Hz48000
                    && control.bitrate >= 64_000
                    && internal_size == 480
                    && self.config.channels == Channels::Stereo
                    && target.iter().all(|&x| x.unsigned_abs() <= 2);
                let slopes = if quiet && eligible && control.bitrate <= 64_000 {
                    &[0, -3, 3][..]
                } else {
                    &[0][..]
                };
                let biases = if ending {
                    &[0, -1, -2, -3, -6, -12, -24, 3, 6, 12][..]
                } else if quiet {
                    &[0, -1, 1, -2, 2][..]
                } else {
                    &[0, -1, -2, -3, -6][..]
                };
                // Compare spectrally tilted energy targets near the PCM quantization floor.
                // Keep the flat target first and score each finalized packet at equal rate.
                for (slope, db) in slopes
                    .iter()
                    .flat_map(|&s| biases.iter().map(move |&d| (s, d)))
                {
                    if db != 0 && !eligible {
                        continue;
                    }
                    #[cfg(test)]
                    let phase = std::time::Instant::now();
                    let mut trial = prepared_state.as_ref().unwrap_or(self).clone();
                    #[cfg(test)]
                    {
                        phases[1] += phase.elapsed().as_nanos();
                    }
                    #[cfg(test)]
                    if eligible && RD_UNFACTORED_REFERENCE.get() {
                        trial.native_prefix_score =
                            Some(hybrid::NativePrefixScore::uncached(&prefix_origin, &target));
                    }
                    trial.hybrid_energy_db = db;
                    trial.hybrid_tilt_db = slope;
                    trial.quick_prefix = quick;
                    trial.proposal_amp = amp;
                    trial.practical_prefix_bitrate = Some(budget);
                    #[cfg(test)]
                    let phase = std::time::Instant::now();
                    let packet = prepared_frame.as_ref().map_or_else(
                        || {
                            if eligible || departing {
                                trial.encode_practical_boundary(
                                    pcm,
                                    EncoderControl {
                                        mode: if departing {
                                            EncoderMode::Celt
                                        } else {
                                            EncoderMode::Hybrid
                                        },
                                        ..control
                                    },
                                )
                            } else {
                                trial.encode_frame_inner(pcm)
                            }
                        },
                        |prepared| {
                            let mut suffix = prepared.clone();
                            for channel in 0..suffix.channels {
                                for band in suffix.start..suffix.end {
                                    suffix.target_energy[channel * 21 + band] +=
                                        (db + slope * (band as i32 - 18)) * 10_885;
                                }
                            }
                            Self::encode_celt_suffix(suffix, None)
                        },
                    );
                    #[cfg(test)]
                    {
                        phases[2] += phase.elapsed().as_nanos();
                    }
                    let packet = match packet {
                        Ok(packet) => packet,
                        Err(error) => {
                            last_error = Some(error);
                            continue;
                        }
                    };
                    #[cfg(test)]
                    if let Some(scorer) = &trial.native_prefix_score {
                        scorer.verify(&packet[1..])?;
                    }
                    #[cfg(test)]
                    if prepared_frame.is_some() && RD_VERIFY_CANDIDATES.get() {
                        let mut reference = self.clone();
                        // Rebuild from the same pre-frame synthesis state and target;
                        // omitting the scorer selects a different SILK encoder.
                        reference.native_prefix_score =
                            Some(hybrid::NativePrefixScore::uncached(&prefix_origin, &target));
                        reference.hybrid_energy_db = db;
                        reference.hybrid_tilt_db = slope;
                        reference.quick_prefix = quick;
                        reference.proposal_amp = amp;
                        reference.practical_prefix_bitrate = Some(budget);
                        let expected = reference.encode_practical_boundary(
                            pcm,
                            EncoderControl {
                                mode: EncoderMode::Hybrid,
                                ..control
                            },
                        )?;
                        assert_eq!(
                            packet, expected,
                            "prepared suffix differs for energy bias {db}"
                        );
                        let mut actual_synthesis = shadow.decoder.clone();
                        let mut expected_synthesis = shadow.decoder.clone();
                        let mut actual_pcm = vec![0; target.len()];
                        let mut expected_pcm = actual_pcm.clone();
                        actual_synthesis.decode(Some(&packet), &mut actual_pcm, false)?;
                        expected_synthesis.decode(Some(&expected), &mut expected_pcm, false)?;
                        assert_eq!(actual_pcm, expected_pcm, "candidate synthesis bias {db}");
                    }
                    if packet.len() > control.bitrate as usize * internal_size / 384_000 {
                        continue;
                    }
                    #[cfg(test)]
                    let phase = std::time::Instant::now();
                    let mut decoder = shadow.decoder.clone();
                    let mut output = vec![0_i16; target.len()];
                    let Ok(samples) = decoder.decode(Some(&packet), &mut output, false) else {
                        continue;
                    };
                    if samples * channel_count(self.config.channels) != target.len() {
                        continue;
                    }
                    let error = hybrid::release_error(&output, &target, split);
                    #[cfg(test)]
                    {
                        phases[3] += phase.elapsed().as_nanos();
                    }
                    #[cfg(test)]
                    if std::env::var_os("MOOSICBOX_OPUS_RD_TRACE").is_some() {
                        eprintln!(
                            "practical RD mode={:?} bitrate={} bias={db} distortion={error} target_energy={} bytes={}",
                            control.mode,
                            control.bitrate,
                            target.iter().map(|&x| i128::from(x).pow(2)).sum::<i128>(),
                            packet.len()
                        );
                    }
                    #[cfg(test)]
                    if hybrid::POOL_DIAGNOSE.get() > 0 {
                        eprintln!(
                            "POOL_FINAL_PACKET budget={budget} bias={db} error={error} bytes={}",
                            packet.len()
                        );
                    }
                    if best.as_ref().is_none_or(|old| error < old.error) {
                        best = Some(PracticalCandidate {
                            #[cfg(test)]
                            bias: db,
                            error,
                            encoder: trial,
                            synthesis: decoder,
                            packet,
                        });
                    }
                }
            }
        }
        #[cfg(test)]
        if std::env::var_os("MOOSICBOX_OPUS_RD_TRACE").is_some()
            && let Some(candidate) = &best
        {
            eprintln!(
                "practical RD winner bias={} distortion={} bitrate={}",
                candidate.bias, candidate.error, control.bitrate
            );
        }
        #[cfg(test)]
        if std::env::var_os("MOOSICBOX_OPUS_RD_PHASE_PROFILE").is_some() {
            prefix_cache.report_costs();
            eprintln!(
                "RD_PHASE bitrate={} rate={:?} channels={:?} size={} prefix_ns={} clone_ns={} suffix_ns={} decode_sse_ns={} cache={:?}",
                control.bitrate,
                self.config.sample_rate,
                self.config.channels,
                self.config.frame_size,
                phases[0],
                phases[1],
                phases[2],
                phases[3],
                prefix_cache.sizes()
            );
        }
        let Some(PracticalCandidate {
            encoder: mut winner,
            synthesis: decoder,
            packet,
            ..
        }) = best
        else {
            return Err(last_error.unwrap_or_else(|| {
                adaptive::unavailable("no finalized practical hybrid RD candidate")
            }));
        };
        shadow.decoder = decoder;
        #[cfg(test)]
        if let Some(scorer) = &winner.native_prefix_score {
            if let Some(previous) = shadow.pending_prefix.take() {
                let (mut context, mut actual, mut replay) = *previous;
                context.target.resize(target.len(), 0);
                let config = scorer.best.as_ref().unwrap().2;
                assert_eq!(
                    context.decode(&mut actual, config, &packet[1..])?,
                    context.decode(&mut replay, config, &packet[1..])?,
                    "actual following Hybrid packet prefix synthesis"
                );
                eprintln!(
                    "actual following Hybrid prefix identity PASS channels={:?}",
                    config.channels
                );
            }
            let (actual, replay) = scorer.branches(&packet[1..])?;
            shadow.pending_prefix = Some(Box::new((scorer.as_ref().clone(), actual, replay)));
        } else {
            shadow.pending_prefix = None;
        }
        winner.hybrid_energy_db = 0;
        winner.quick_prefix = false;
        winner.proposal_amp = 1.;
        winner.hybrid_tilt_db = 0;
        winner.practical_prefix_bitrate = None;
        winner.native_prefix_score = None;
        let state = winner
            .adaptive
            .as_mut()
            .ok_or_else(|| adaptive::unavailable("RD winner missing adaptive state"))?;
        state.control = control;
        winner.practical = Some(shadow);
        *self = winner;
        Ok(packet)
    }

    #[allow(clippy::too_many_lines)] // Ordered CELT packet entropy stages.
    fn encode_frame_inner(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
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
        self.encode_practical_boundary(pcm, control)
    }

    #[cfg(feature = "hybrid")]
    fn encode_practical_boundary(
        &mut self,
        pcm: &[f32],
        control: EncoderControl,
    ) -> Result<Vec<u8>> {
        self.encode_practical_boundary_prepared(pcm, control, None)
    }

    #[cfg(feature = "hybrid")]
    fn upsample_hybrid_input(
        input: &mut Option<resample::Interpolator>,
        pcm: &[f32],
    ) -> Option<Vec<f32>> {
        input.as_mut().map(|input| {
            input
                .process(pcm)
                .into_iter()
                .map(|sample| (f64::from(sample) / 134_217_728.0) as f32)
                .collect()
        })
    }

    #[cfg(feature = "hybrid")]
    fn encode_practical_boundary_prepared(
        &mut self,
        pcm: &[f32],
        control: EncoderControl,
        capture: Option<&mut Option<PreparedCeltFrame>>,
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
        state.end = hybrid::coding_layout(
            control.bandwidth,
            self.config.channels,
            self.config.frame_size * rate_factor(self.config.sample_rate),
        )?
        .end_band;
        let internal_size = self.config.frame_size * rate_factor(self.config.sample_rate);
        let upsampled = Self::upsample_hybrid_input(&mut self.hybrid_input, pcm);
        let pcm = upsampled.as_deref().unwrap_or(pcm);
        let low = self.hybrid.as_mut().expect("hybrid test filters");
        self.adaptive_low = low.process(pcm);
        for x in &mut self.adaptive_low {
            *x *= self.proposal_amp;
        }
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
        // At minimum rate the unscored short prefix needs excitation bits;
        // reserve two suffix bytes. At 64k mode entry reserve half for CELT.
        // An explicit allocation is final, not an input to this default policy.
        let default_rate = match (
            self.config.sample_rate,
            self.config.channels,
            internal_size,
            target,
            control.bitrate,
        ) {
            (SampleRate::Hz48000, Channels::Stereo, 480, EncoderMode::Hybrid, 32_000) => 30_400,
            (SampleRate::Hz48000, Channels::Stereo, 480, EncoderMode::Hybrid, 64_000)
                if previous != target =>
            {
                32_000
            }
            _ => {
                hybrid::packet_lowband_bitrate(control.bitrate, self.config.channels, internal_size)
            }
        };
        self.silk
            .as_mut()
            .unwrap()
            .set_bitrate(self.practical_prefix_bitrate.unwrap_or(default_rate))?;
        let packet = self.encode_coding_frame_prepared(&delayed, None, capture)?;
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
        let end = if packet_mode == EncoderMode::Silk {
            17
        } else {
            hybrid::coding_layout(
                self.adaptive.as_ref().unwrap().control.bandwidth,
                self.config.channels,
                self.config.frame_size * rate_factor(self.config.sample_rate),
            )?
            .end_band
        };
        self.adaptive.as_mut().unwrap().end = end;
        // Both analysis branches advance once, regardless of the selected packet mode.
        // Their fixed delays are identical to explicit hybrid (384 samples).
        let upsampled = Self::upsample_hybrid_input(&mut self.hybrid_input, pcm);
        let pcm = upsampled.as_deref().unwrap_or(pcm);
        let low = self
            .hybrid
            .as_mut()
            .ok_or_else(|| adaptive::unavailable("missing adaptive filters"))?;
        self.adaptive_low = low.process(pcm);
        for x in &mut self.adaptive_low {
            *x *= self.proposal_amp;
        }
        let delayed = low.delay_celt(pcm);
        // Conversion already produced 48 kHz PCM: use its actual clock for
        // all CELT calls, including redundancy, then restore the public clock.
        let input_config = self.config;
        self.config.frame_size *= rate_factor(input_config.sample_rate);
        self.config.sample_rate = SampleRate::Hz48000;
        let result = self.encode_adaptive_internal(
            &delayed,
            active,
            target,
            packet_mode,
            entering,
            departing,
        );
        self.config = input_config;
        result
    }

    #[cfg(feature = "hybrid")]
    #[allow(clippy::too_many_arguments)] // Boundary decisions are computed before input-clock conversion.
    fn encode_adaptive_internal(
        &mut self,
        delayed: &[f32],
        active: EncoderMode,
        target: EncoderMode,
        packet_mode: EncoderMode,
        entering: bool,
        departing: bool,
    ) -> Result<Vec<u8>> {
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
                Some(self.encode_redundancy(delayed, true)?)
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
            self.encode_coding_frame(delayed, redundancy.as_deref().map(|data| (entering, data)))?
        };
        if departing {
            // The redundant tail primes the following CELT packet. Publish the
            // change only on the next frame: this packet still carries hybrid TOC.
            self.adaptive.as_mut().unwrap().next_celt = true;
            self.prime_redundant_tail(delayed)?;
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
        self.encode_coding_frame_prepared(pcm, redundant, None)
    }

    #[allow(clippy::too_many_lines)] // Ordered packet preparation includes unchanged SILK retry/signaling.
    fn encode_coding_frame_prepared(
        &mut self,
        pcm: &[f32],
        redundant: Option<(bool, &[u8])>,
        capture: Option<&mut Option<PreparedCeltFrame>>,
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
        let quantized;
        let analysis_pcm = if hybrid
            && self.config.channels == Channels::Stereo
            && self.config.sample_rate == SampleRate::Hz48000
            && self.config.bitrate >= 64_000
            && self.config.frame_size == 480
        {
            quantized = analysis_pcm
                .iter()
                .map(|&v| (v * 32768.0).round() / 32768.0)
                .collect::<Vec<_>>();
            &quantized[..]
        } else {
            analysis_pcm
        };
        let resampled = self.interpolator.process(analysis_pcm);
        let internal_size = self.config.frame_size * rate_factor(self.config.sample_rate);
        let bytes = (self.config.bitrate as usize * internal_size / 384_000)
            .saturating_sub(1 + redundant.map_or(0, |(_, data)| data.len()));
        let (spectrum, transient) = self
            .analysis
            .transform_celt(&resampled, !hybrid && bytes >= 24 * channels);
        let lm = (internal_size / 120).ilog2() as usize;
        #[cfg_attr(not(feature = "hybrid"), allow(unused_mut))]
        let (mut target_energy, energy) = quantize::energies(&spectrum, lm);
        #[cfg(feature = "hybrid")]
        if hybrid && (self.hybrid_energy_db != 0 || self.hybrid_tilt_db != 0) {
            let bias = self.hybrid_energy_db;
            for channel in 0..channels {
                for band in start..end {
                    target_energy[channel * 21 + band] +=
                        (bias + self.hybrid_tilt_db * (band as i32 - 18)) * 10_885;
                }
            }
        }
        #[cfg(test)]
        let (target_energy, energy) = {
            let mut target_energy = target_energy;
            if hybrid && self.config.bitrate < 128_000 {
                // Legal log2-amplitude energy targets: 1 dB = log2(10)/20.
                // Both coarse coding and fine/final coding use this same target.
                let offset = HYBRID_TEST_ENERGY_DB.get() * 10_885;
                for channel in 0..channels {
                    for band in start..end {
                        target_energy[channel * 21 + band] += offset;
                    }
                }
            }
            (target_energy, energy)
        };
        #[cfg_attr(not(feature = "hybrid"), allow(unused_mut))]
        let mut enc = RangeEncoder::new(bytes).map_err(entropy)?;
        #[cfg(feature = "hybrid")]
        if hybrid && let Some(lowband) = &mut self.hybrid {
            let low = if self.adaptive.is_some() {
                self.adaptive_low.clone()
            } else {
                lowband.process(pcm)
            };
            let silk = self
                .silk
                .as_mut()
                .ok_or_else(|| Error::Unsupported("missing hybrid SILK state".into()))?;
            if redundant.is_some() {
                // The redundancy payload has already been removed from `bytes`.
                // Give SILK only the main-coder capacity left after signaling,
                // before searching or mutating its predictive state.
                let prefix_bytes = (bytes * 8).saturating_sub(enc.tell() as usize + 37) / 8;
                let rate = ((prefix_bytes + 1) * 384_000 / internal_size) as u32;
                let nominal = hybrid::packet_lowband_bitrate(
                    self.config.bitrate,
                    self.config.channels,
                    internal_size,
                );
                silk.set_bitrate(rate.clamp(8_000, 128_000).min(nominal))?;
            }
            let original = silk.clone();
            let incoming = enc.clone();
            if let Some(scorer) = &mut self.native_prefix_score {
                // Preserve full-rate SILK prediction history across subsequent rate changes.
                let refine = self.config.sample_rate == SampleRate::Hz48000
                    && self.config.channels == Channels::Stereo
                    && self.config.bitrate < 64_000
                    && internal_size == 480
                    && scorer.target.iter().all(|v| v.unsigned_abs() < 128);
                let rate_control = internal_size == 480
                    && (self.quick_prefix
                        || (self.config.sample_rate == SampleRate::Hz48000
                            && (64_000..128_000).contains(&self.config.bitrate)));
                silk.prefix_search = if rate_control || self.proposal_amp == 0.0 {
                    silk::PrefixSearch::Rate
                } else if refine {
                    silk::PrefixSearch::QuietRefine
                } else {
                    silk::PrefixSearch::Full
                };
                // At short packet budgets the refined NLSF can consume the bits
                // needed for excitation. Offer its coarse alternative only at
                // low levels or for strongly alternating low-band samples.
                let compact = internal_size == 480
                    && self.config.channels == Channels::Stereo
                    && self.config.sample_rate == SampleRate::Hz48000
                    && self.config.bitrate == 64_000
                    && scorer.target.iter().any(|v| v.unsigned_abs() > 2)
                    && {
                        let power = low.iter().map(|x| x * x).sum::<f32>();
                        let adjacent = low
                            .iter()
                            .zip(low.iter().skip(2))
                            .map(|(x, y)| x * y)
                            .sum::<f32>();
                        power > 0.0
                            && (scorer.target.iter().all(|v| v.unsigned_abs() <= 256)
                                || adjacent < -0.7 * power)
                    };
                silk.set_compact_prefix(compact);
                if self.config.channels == Channels::Stereo
                    && internal_size == 480
                    && self.config.bitrate == 32_000
                    && self.proposal_amp.to_bits() == 1f32.to_bits()
                {
                    // Bounded stereo uses the rate only for analysis (24/32k
                    // per channel) and pulse cost/shaping (24k). The actual
                    // budget, feasibility checks and ranking remain per trial.
                    let rate = silk.bitrate() / 2;
                    let class = u32::from(rate >= 24000) + u32::from(rate >= 32000);
                    let mut workspace = scorer.cache.quantizers.remove(&class).unwrap_or_default();
                    // The fallback search has its own rate-dependent trial costs.
                    workspace.clear_fallback();
                    silk.encode_prefix_cached(&low, &mut enc, scorer.as_mut(), &mut workspace)?;
                    scorer.cache.quantizers.insert(class, workspace);
                } else {
                    silk.encode_prefix_scored(&low, &mut enc, scorer.as_mut())?;
                }
                #[cfg(test)]
                scorer.verify(&enc.clone().finish().map_err(entropy)?)?;
            } else if redundant.is_some() {
                silk.encode_prefix_ranked(&low, &mut enc)?;
            } else {
                hybrid::encode_lowband_prefix(silk, &low, &mut enc, self.config.bitrate)?;
            }

            if redundant.is_some() && enc.tell() as usize + 37 > bytes * 8 {
                // Redundancy reduces storage without reducing the nominal SILK
                // budget. Retry from the incoming predictive/entropy state, not
                // from the failed trial, reserving the signaling bits explicitly.
                let prefix_bytes = (bytes * 8).saturating_sub(incoming.tell() as usize + 37) / 8;
                let retry_bitrate = ((prefix_bytes + 1) * 384_000 / internal_size) as u32;
                let mut trial = original;
                trial.set_bitrate(retry_bitrate.clamp(8_000, 128_000))?;
                enc = incoming;
                trial.encode_prefix_ranked(&low, &mut enc)?;
                if enc.tell() as usize + 37 > bytes * 8 {
                    return Err(adaptive::unavailable(
                        "hybrid transition cannot fit redundancy signaling",
                    ));
                }
                trial.set_bitrate(hybrid::packet_lowband_bitrate(
                    self.config.bitrate,
                    self.config.channels,
                    internal_size,
                ))?;
                *silk = trial;
            }
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
        let prepared = PreparedCeltFrame {
            #[cfg(feature = "hybrid")]
            joint_hybrid: self
                .silk
                .as_ref()
                .is_some_and(silk::SilkEncoder::compact_prefix),
            #[cfg(not(feature = "hybrid"))]
            joint_hybrid: false,
            range: enc,
            spectrum,
            target_energy,
            energy,
            transient,
            hybrid,
            channels,
            lm,
            bytes,
            bitrate: self.config.bitrate,
            start,
            end,
        };
        if let Some(capture) = capture {
            *capture = Some(prepared);
            return Ok(Vec::new());
        }
        Self::encode_celt_suffix(prepared, redundant)
    }

    #[allow(clippy::too_many_lines)] // Ordered CELT suffix entropy stages, shared by legacy and RD paths.
    fn encode_celt_suffix(
        prepared: PreparedCeltFrame,
        redundant: Option<(bool, &[u8])>,
    ) -> Result<Vec<u8>> {
        let PreparedCeltFrame {
            joint_hybrid,
            range: mut enc,
            spectrum,
            target_energy,
            mut energy,
            transient,
            hybrid,
            channels,
            lm,
            bytes,
            bitrate,
            start,
            end,
        } = prepared;
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
        let intensity = if channels == 2 && bitrate <= 40_000 {
            13
        } else if channels == 2 && bitrate <= 64_000 {
            17
        } else {
            21
        };
        let intensity = allocation::stereo_intensity(&spectrum, lm, intensity);
        let allocation = if joint_hybrid && hybrid && channels == 2 && bitrate <= 64_000 && lm == 2
        {
            allocation::allocate_joint(
                &mut enc,
                remaining,
                channels,
                lm as i32,
                start,
                end,
                intensity.min(end),
            )?
        } else if hybrid && end == 21 {
            allocation::allocate(&mut enc, remaining, channels, lm as i32, start)?
        } else if hybrid {
            allocation::allocate_celt_end(
                &mut enc, remaining, channels, lm as i32, start, end, end,
            )?
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
        let bandwidth_base = if end == 19 { 0xc0 } else { 0xe0 };
        packet.push(
            (if hybrid {
                (if end == 19 { 0x60 } else { 0x70 }) | ((u8::from(lm == 3)) << 3)
            } else {
                bandwidth_base | ((lm as u8) << 3)
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
