//! Explicit backend selection and gapless Ogg Opus stream encoding.

use moosicbox_ogg_opus::{OggOpusWriter, OpusHead, OpusTags};
#[cfg(feature = "opus-native")]
pub use moosicbox_opus_native::encoder::{EncoderControl, EncoderMode};
#[cfg(feature = "opus-native")]
pub use moosicbox_opus_native::multistream::MultistreamLayout;
use std::io::{self, Write};

/// Requested encoder implementation. Unavailable backends never fall back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpusEncodeBackend {
    /// Compatibility C libopus encoder.
    #[default]
    Libopus,
    /// Pure Rust encoder.
    Native,
}

/// Requested packet coding mode. Adaptation requires explicit `Auto` selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpusEncodeMode {
    /// Preserve the backend's default: native CELT or libopus automatic selection.
    #[default]
    Default,
    /// Opt in to qualified native frame-boundary adaptive controls.
    Auto,
    /// Force native CELT transform coding (2.5/5/10/20 ms; minimum bitrate
    /// 128/64/32/32 kbps respectively, maximum 256 kbps).
    Celt,
    /// Force native SILK predictive coding (8/12/16 kHz, 10/20 ms;
    /// mono 48..=128 kbps, stereo 96..=128 kbps).
    Silk,
    /// Force native hybrid coding (48 kHz, 10/20 ms, 128..=256 kbps).
    Hybrid,
}

/// Configuration for interleaved Opus PCM encoding.
///
/// Default and explicit modes remain fixed; `Auto` opts in to qualified native
/// adaptation. No C fallback is used. Pass this configuration through `SignalChainStep::with_opus_encoder`
/// or the files crate's `get_audio_bytes_with_opus_encoder`.
/// Packet durations, pre-skip and EOS granules use the container's 48-kHz clock.
#[derive(Clone, Copy, Debug)]
pub struct OpusEncodeConfig {
    /// Encoder implementation.
    pub backend: OpusEncodeBackend,
    /// Explicit coding mode. Forced modes require the native backend.
    pub mode: OpusEncodeMode,
    /// Input sample rate in Hz.
    pub sample_rate: u32,
    /// Number of interleaved channels (one or two).
    pub channels: u8,
    /// Target bitrate in bits per second.
    pub bitrate: u32,
    /// Samples per channel per packet at `sample_rate` (2.5/5/10/20 ms,
    /// or 40/60 ms for explicit native SILK only).
    pub frame_size: usize,
}
impl Default for OpusEncodeConfig {
    fn default() -> Self {
        Self {
            backend: OpusEncodeBackend::Libopus,
            mode: OpusEncodeMode::Default,
            sample_rate: 48_000,
            channels: 2,
            bitrate: 128_000,
            frame_size: 960,
        }
    }
}

/// Explicit native multistream Ogg configuration. Bitrates are per elementary stream.
#[cfg(feature = "opus-native")]
#[derive(Clone, Debug)]
pub struct OpusMultistreamEncodeConfig {
    /// Input rate shared by all streams.
    pub sample_rate: u32,
    /// Samples per channel per packet.
    pub frame_size: usize,
    /// Explicit application-channel mapping.
    pub layout: moosicbox_opus_native::multistream::MultistreamLayout,
    /// Per-stream CELT bitrates, coupled streams first.
    pub bitrates: Vec<u32>,
    /// Ogg mapping family: canonical Vorbis order (1) or explicit mapping (255).
    pub mapping_family: u8,
}

#[cfg(feature = "opus-native")]
impl OpusMultistreamEncodeConfig {
    fn validate(&self) -> io::Result<()> {
        if self.bitrates.len() != usize::from(self.layout.streams()) {
            return Err(invalid("multistream bitrate count must match stream count"));
        }
        match self.mapping_family {
            255 => Ok(()),
            1 => {
                let expected = OpusHead::new_family1(
                    u8::try_from(self.layout.channels()).map_err(invalid)?,
                    0,
                    self.sample_rate,
                )?;
                if self.layout.streams() != expected.stream_count
                    || self.layout.coupled_streams() != expected.coupled_count
                    || self.layout.mapping() != expected.channel_mapping
                {
                    return Err(invalid(
                        "family 1 requires canonical stream layout and channel order",
                    ));
                }
                Ok(())
            }
            _ => Err(invalid("multistream requires mapping family 1 or 255")),
        }
    }
}

enum Backend {
    #[cfg(feature = "opus-native")]
    Multistream(moosicbox_opus_native::multistream::MultistreamEncoder),
    #[cfg(feature = "opus-native")]
    Native(Box<moosicbox_opus_native::encoder::Encoder>),
    #[cfg(feature = "opus")]
    Libopus(std::sync::Mutex<opus::Encoder>),
}

/// Streaming encoder retaining partial PCM until finalization.
/// The final Ogg granule excludes both encoder delay and zero-padded tail samples.
pub struct OpusStreamEncoder<W: Write> {
    config: OpusEncodeConfig,
    backend: Backend,
    writer: Option<OggOpusWriter<W>>,
    pending: Vec<f32>,
    audible: u64,
    encoded: u64,
    lookahead: u64,
    failure: Option<(io::ErrorKind, String)>,
}

fn invalid(message: impl std::fmt::Display) -> io::Error {
    io::Error::other(message.to_string())
}

impl<W: Write> OpusStreamEncoder<W> {
    /// Construct an encoder and write the Ogg identification/comment headers.
    ///
    /// # Errors
    /// * Rejects unsupported configurations or unavailable implementations.
    /// * Propagates codec initialization, header, and output errors.
    #[allow(clippy::too_many_lines)] // Keep backend initialization and header validation atomic.
    pub fn new(sink: W, config: OpusEncodeConfig) -> io::Result<Self> {
        let long_silk_frame = config.backend == OpusEncodeBackend::Native
            && config.mode == OpusEncodeMode::Silk
            && [
                config.sample_rate as usize / 25,
                config.sample_rate as usize * 3 / 50,
            ]
            .contains(&config.frame_size);
        if ![8_000, 12_000, 16_000, 24_000, 48_000].contains(&config.sample_rate)
            || !(1..=2).contains(&config.channels)
            || (!long_silk_frame
                && ![
                    config.sample_rate as usize / 400,
                    config.sample_rate as usize / 200,
                    config.sample_rate as usize / 100,
                    config.sample_rate as usize / 50,
                ]
                .contains(&config.frame_size))
        {
            return Err(invalid(
                "Opus stream requires 8/12/16/24/48 kHz, mono/stereo, and 2.5/5/10/20 ms frames (40/60 ms only for native SILK)",
            ));
        }
        if config.backend == OpusEncodeBackend::Libopus && config.mode != OpusEncodeMode::Default {
            return Err(invalid(
                "forced Opus coding modes require the native backend",
            ));
        }
        let clock_ratio = u64::from(48_000 / config.sample_rate);
        let (backend, lookahead) = match config.backend {
            OpusEncodeBackend::Native => {
                #[cfg(feature = "opus-native")]
                {
                    use moosicbox_opus_native::{
                        Channels, SampleRate,
                        encoder::{Encoder, EncoderConfig},
                    };
                    let codec_config = EncoderConfig {
                        sample_rate: SampleRate::from_hz(config.sample_rate).map_err(invalid)?,
                        channels: if config.channels == 1 {
                            Channels::Mono
                        } else {
                            Channels::Stereo
                        },
                        bitrate: config.bitrate,
                        frame_size: config.frame_size,
                    };
                    let encoder = match config.mode {
                        OpusEncodeMode::Default | OpusEncodeMode::Celt => {
                            Encoder::new(codec_config)
                        }
                        OpusEncodeMode::Auto => Encoder::new_auto(codec_config),
                        OpusEncodeMode::Silk => Encoder::new_silk(codec_config),
                        OpusEncodeMode::Hybrid => Encoder::new_hybrid(codec_config),
                    }
                    .map_err(invalid)?;
                    let lookahead = encoder.lookahead();
                    (Backend::Native(Box::new(encoder)), lookahead)
                }
                #[cfg(not(feature = "opus-native"))]
                return Err(invalid("requested native Opus encoder is not enabled"));
            }
            OpusEncodeBackend::Libopus => {
                #[cfg(feature = "opus")]
                {
                    let mut encoder = opus::Encoder::new(
                        config.sample_rate,
                        if config.channels == 1 {
                            opus::Channels::Mono
                        } else {
                            opus::Channels::Stereo
                        },
                        opus::Application::Audio,
                    )
                    .map_err(invalid)?;
                    encoder
                        .set_bitrate(opus::Bitrate::Bits(
                            i32::try_from(config.bitrate).map_err(invalid)?,
                        ))
                        .map_err(invalid)?;
                    let lookahead = usize::try_from(encoder.get_lookahead().map_err(invalid)?)
                        .map_err(invalid)?;
                    (Backend::Libopus(std::sync::Mutex::new(encoder)), lookahead)
                }
                #[cfg(not(feature = "opus"))]
                return Err(invalid("requested libopus encoder is not enabled"));
            }
        };
        let lookahead = u64::try_from(lookahead)
            .map_err(invalid)?
            .checked_mul(clock_ratio)
            .ok_or_else(|| invalid("Opus lookahead overflow"))?;
        let head = OpusHead::new(
            config.channels,
            u16::try_from(lookahead).map_err(invalid)?,
            config.sample_rate,
        )?;
        let writer = OggOpusWriter::new(
            sink,
            head,
            &OpusTags {
                vendor: b"MoosicBox".to_vec(),
                comments: Vec::new(),
            },
            1,
        )?;
        Ok(Self {
            config,
            backend,
            writer: Some(writer),
            pending: Vec::new(),
            audible: 0,
            encoded: 0,
            lookahead,
            failure: None,
        })
    }
    /// Construct a native CELT multistream encoder with an explicit Ogg mapping.
    ///
    /// # Errors
    /// * Rejects invalid family/layout, stream bitrates, rates, frames or delays before headers.
    /// * Propagates native initialization and output errors. No C fallback is used.
    #[cfg(feature = "opus-native")]
    pub fn new_multistream(sink: W, config: OpusMultistreamEncodeConfig) -> io::Result<Self> {
        use moosicbox_opus_native::{
            Channels, SampleRate, encoder::EncoderConfig, multistream::MultistreamEncoder,
        };
        config.validate()?;
        let sample_rate = SampleRate::from_hz(config.sample_rate).map_err(invalid)?;
        let configs = config
            .bitrates
            .iter()
            .enumerate()
            .map(|(stream, &bitrate)| EncoderConfig {
                sample_rate,
                channels: if stream < usize::from(config.layout.coupled_streams()) {
                    Channels::Stereo
                } else {
                    Channels::Mono
                },
                bitrate,
                frame_size: config.frame_size,
            })
            .collect();
        let stream_count = config.layout.streams();
        let coupled_count = config.layout.coupled_streams();
        let channel_mapping = config.layout.mapping().to_vec();
        let encoder = MultistreamEncoder::new(config.layout, configs).map_err(invalid)?;
        let lookahead = u64::try_from(encoder.lookahead()).map_err(invalid)?
            * u64::from(48_000 / config.sample_rate);
        let channels = u8::try_from(channel_mapping.len()).map_err(invalid)?;
        let bitrate = config
            .bitrates
            .iter()
            .try_fold(0u32, |total, &rate| total.checked_add(rate))
            .ok_or_else(|| invalid("multistream total bitrate overflow"))?;
        let head = OpusHead {
            channels,
            pre_skip: u16::try_from(lookahead).map_err(invalid)?,
            input_sample_rate: config.sample_rate,
            output_gain: 0,
            mapping_family: config.mapping_family,
            stream_count,
            coupled_count,
            channel_mapping,
        };
        let writer = OggOpusWriter::new(
            sink,
            head,
            &OpusTags {
                vendor: b"MoosicBox".to_vec(),
                comments: Vec::new(),
            },
            1,
        )?;
        Ok(Self {
            config: OpusEncodeConfig {
                backend: OpusEncodeBackend::Native,
                mode: OpusEncodeMode::Celt,
                sample_rate: config.sample_rate,
                channels,
                bitrate,
                frame_size: config.frame_size,
            },
            backend: Backend::Multistream(encoder),
            writer: Some(writer),
            pending: Vec::new(),
            audible: 0,
            encoded: 0,
            lookahead,
            failure: None,
        })
    }

    /// Return the selected implementation.
    #[must_use]
    pub const fn backend(&self) -> OpusEncodeBackend {
        self.config.backend
    }
    /// Return the input configuration.
    #[must_use]
    pub const fn config(&self) -> OpusEncodeConfig {
        self.config
    }
    /// Apply native runtime controls only between complete PCM frames.
    ///
    /// # Errors
    /// * Rejects finished/failed streams, partial PCM frames, and nonnative backends.
    /// * Propagates unsupported native controls without consuming PCM.
    #[cfg(feature = "opus-native")]
    pub fn set_control(
        &mut self,
        control: moosicbox_opus_native::encoder::EncoderControl,
    ) -> io::Result<()> {
        self.check_failure()?;
        if self.writer.is_none() || !self.pending.is_empty() {
            return Err(invalid(
                "Opus controls require an open stream at a frame boundary",
            ));
        }
        match &mut self.backend {
            Backend::Multistream(_) => {
                return Err(invalid("multistream runtime controls are not supported"));
            }
            Backend::Native(encoder) => encoder.set_control(control).map_err(invalid)?,
            #[cfg(feature = "opus")]
            Backend::Libopus(_) => {
                return Err(invalid("native controls require the native backend"));
            }
        }
        self.config.bitrate = control.bitrate;
        Ok(())
    }

    /// Return native requested controls, or `None` for a nonnative backend.
    #[cfg(feature = "opus-native")]
    pub fn control(&self) -> Option<moosicbox_opus_native::encoder::EncoderControl> {
        match &self.backend {
            Backend::Multistream(_) => None,
            Backend::Native(encoder) => Some(encoder.control()),
            #[cfg(feature = "opus")]
            Backend::Libopus(_) => None,
        }
    }

    /// Return the native active packet mode, or `None` for a nonnative backend.
    #[cfg(feature = "opus-native")]
    pub const fn active_mode(&self) -> Option<moosicbox_opus_native::encoder::EncoderMode> {
        match &self.backend {
            Backend::Multistream(_) => Some(moosicbox_opus_native::encoder::EncoderMode::Celt),
            Backend::Native(encoder) => Some((**encoder).active_mode()),
            #[cfg(feature = "opus")]
            Backend::Libopus(_) => None,
        }
    }

    fn frame(&mut self, pcm: &[f32]) -> io::Result<()> {
        let packet = match &mut self.backend {
            #[cfg(feature = "opus-native")]
            Backend::Multistream(encoder) => encoder.encode(pcm).map_err(invalid)?,
            #[cfg(feature = "opus-native")]
            Backend::Native(encoder) => encoder.encode(pcm).map_err(|error| {
                let peak = pcm
                    .iter()
                    .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
                invalid(format!(
                    "{error}; config={:?}, pcm_samples={}, pcm_peak={peak}",
                    self.config,
                    pcm.len()
                ))
            })?,
            #[cfg(feature = "opus")]
            Backend::Libopus(encoder) => {
                let mut output = vec![0; 4000];
                let len = encoder
                    .get_mut()
                    .map_err(invalid)?
                    .encode_float(pcm, &mut output)
                    .map_err(invalid)?;
                output.truncate(len);
                output
            }
        };
        self.encoded += u64::from(moosicbox_ogg_opus::packet_duration(&packet)?);
        self.writer
            .as_mut()
            .ok_or_else(|| invalid("Opus encoder already finished"))?
            .write_packet(&packet)
    }
    /// Append normalized interleaved PCM. Arbitrary whole-channel chunks are accepted.
    ///
    /// # Errors
    /// * Rejects incomplete channel frames, nonfinite PCM, and writes after finish.
    /// * Propagates codec and output failures.
    pub fn write(&mut self, pcm: &[f32]) -> io::Result<()> {
        self.check_failure()?;
        if self.writer.is_none() {
            return Err(invalid("Opus encoder already finished"));
        }
        if !pcm.len().is_multiple_of(usize::from(self.config.channels))
            || pcm.iter().any(|x| !x.is_finite())
        {
            return Err(invalid("invalid interleaved Opus PCM"));
        }
        self.audible = self
            .audible
            .checked_add(
                u64::try_from(pcm.len() / usize::from(self.config.channels))
                    .map_err(invalid)?
                    .checked_mul(u64::from(48_000 / self.config.sample_rate))
                    .ok_or_else(|| invalid("Opus sample clock overflow"))?,
            )
            .ok_or_else(|| invalid("Opus sample count overflow"))?;
        let frame = self.config.frame_size * usize::from(self.config.channels);
        let mut remaining = pcm;
        if !self.pending.is_empty() {
            let count = (frame - self.pending.len()).min(remaining.len());
            self.pending.extend_from_slice(&remaining[..count]);
            remaining = &remaining[count..];
            if self.pending.len() == frame {
                let input = std::mem::take(&mut self.pending);
                if let Err(error) = self.frame(&input) {
                    self.failure = Some((error.kind(), error.to_string()));
                    return Err(error);
                }
            }
        }
        let mut chunks = remaining.chunks_exact(frame);
        for input in &mut chunks {
            if let Err(error) = self.frame(input) {
                self.failure = Some((error.kind(), error.to_string()));
                return Err(error);
            }
        }
        self.pending.extend_from_slice(chunks.remainder());
        Ok(())
    }
    /// Drain codec delay and close the stream with its exact audible sample count.
    /// Subsequent calls return `None` without emitting a second EOS.
    ///
    /// # Errors
    /// * Propagates codec drain, packet validation, and output errors.
    /// * A codec or output failure permanently closes the stream to further work.
    pub fn finish(&mut self) -> io::Result<Option<W>> {
        self.check_failure()?;
        let result = self.finish_inner();
        if let Err(error) = &result {
            self.failure = Some((error.kind(), error.to_string()));
        }
        result
    }

    fn check_failure(&self) -> io::Result<()> {
        match &self.failure {
            Some((kind, message)) => Err(io::Error::new(*kind, message.clone())),
            None => Ok(()),
        }
    }

    fn finish_inner(&mut self) -> io::Result<Option<W>> {
        if self.writer.is_none() {
            return Ok(None);
        }
        if !self.pending.is_empty() {
            let mut input = std::mem::take(&mut self.pending);
            input.resize(
                self.config.frame_size * usize::from(self.config.channels),
                0.0,
            );
            self.frame(&input)?;
        }
        // Even an empty stream needs one packet to carry pre-skip and EOS.
        // The final granule trims this priming packet to zero audible samples.
        if self.encoded == 0 {
            self.frame(&vec![
                0.0;
                self.config.frame_size
                    * usize::from(self.config.channels)
            ])?;
        }
        match &mut self.backend {
            #[cfg(feature = "opus-native")]
            Backend::Multistream(encoder) => {
                for packet in encoder.finish().map_err(invalid)? {
                    if self.encoded < self.audible + self.lookahead {
                        self.encoded += u64::from(moosicbox_ogg_opus::packet_duration(&packet)?);
                        self.writer
                            .as_mut()
                            .ok_or_else(|| invalid("Opus writer unavailable during drain"))?
                            .write_packet(&packet)?;
                    }
                }
            }
            #[cfg(feature = "opus-native")]
            Backend::Native(encoder) => {
                for packet in encoder.finish().map_err(invalid)? {
                    if self.encoded < self.audible + self.lookahead {
                        self.encoded += u64::from(moosicbox_ogg_opus::packet_duration(&packet)?);
                        self.writer
                            .as_mut()
                            .ok_or_else(|| invalid("Opus writer unavailable during drain"))?
                            .write_packet(&packet)?;
                    }
                }
            }
            #[cfg(feature = "opus")]
            Backend::Libopus(_) => {
                // A full zero frame supplies libopus lookahead even when input ends
                // at a packet boundary; EOS granule removes any excess padding.
                while self.encoded < self.audible + self.lookahead {
                    self.frame(&vec![
                        0.0;
                        self.config.frame_size
                            * usize::from(self.config.channels)
                    ])?;
                }
            }
        }
        self.writer
            .take()
            .ok_or_else(|| invalid("Opus writer unavailable during finish"))?
            .finish(self.audible)
            .map(Some)
    }
}
