//! Symphonia adapter for complete Opus packets.

use std::sync::Mutex;

use log::warn;
use symphonia::core::{
    audio::{
        AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec, GenericAudioBufferRef, Position,
    },
    codecs::{
        CodecInfo,
        audio::{
            AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
            well_known::CODEC_ID_OPUS,
        },
        registry::{RegisterableAudioDecoder, SupportedAudioCodec},
    },
    errors::{Error, Result},
    packet::PacketRef,
};

/// Explicit codec implementation selection. Disabled backends return an error;
/// selection never silently falls back to another implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpusBackend {
    /// The reference C implementation (the default when `libopus` is enabled).
    Libopus,
    /// The native Rust implementation, enabled with the `native` feature.
    Native,
}

enum BackendDecoder {
    #[cfg(feature = "libopus")]
    Libopus(audiopus::coder::Decoder),
    #[cfg(feature = "native")]
    Native(
        Box<moosicbox_opus_native::multistream::MultistreamDecoder>,
        usize,
    ),
    Unavailable,
}

#[cfg_attr(
    not(any(feature = "libopus", feature = "native")),
    allow(clippy::missing_const_for_fn, clippy::needless_pass_by_ref_mut)
)]
impl BackendDecoder {
    fn new(
        backend: OpusBackend,
        sample_rate: u32,
        channels: usize,
        head: Option<&moosicbox_ogg_opus::OpusHead>,
    ) -> Result<Self> {
        let _ = head;
        match backend {
            #[cfg(feature = "libopus")]
            OpusBackend::Libopus => {
                if channels > 2 || head.is_some_and(|head| head.mapping_family != 0) {
                    return Err(Error::Unsupported(
                        "libopus multistream adapter unavailable",
                    ));
                }
                let rate = match sample_rate {
                    8000 => audiopus::SampleRate::Hz8000,
                    12000 => audiopus::SampleRate::Hz12000,
                    16000 => audiopus::SampleRate::Hz16000,
                    24000 => audiopus::SampleRate::Hz24000,
                    48000 => audiopus::SampleRate::Hz48000,
                    _ => return Err(Error::Unsupported("unsupported sample rate")),
                };
                let channels = if channels == 1 {
                    audiopus::Channels::Mono
                } else {
                    audiopus::Channels::Stereo
                };
                audiopus::coder::Decoder::new(rate, channels)
                    .map(Self::Libopus)
                    .map_err(|_| Error::DecodeError("failed to create libopus decoder"))
            }
            #[cfg(feature = "native")]
            OpusBackend::Native => {
                use moosicbox_opus_native::multistream::{MultistreamDecoder, MultistreamLayout};

                let rate = moosicbox_opus_native::SampleRate::from_hz(sample_rate)
                    .map_err(|_| Error::Unsupported("unsupported sample rate"))?;
                let (streams, coupled, mapping) =
                    head.filter(|head| head.mapping_family == 1).map_or_else(
                        || {
                            (
                                1,
                                u8::from(channels == 2),
                                (0..u8::try_from(channels).unwrap_or(0)).collect(),
                            )
                        },
                        |head| {
                            (
                                head.stream_count,
                                head.coupled_count,
                                head.channel_mapping.clone(),
                            )
                        },
                    );
                let layout = MultistreamLayout::new(streams, coupled, mapping)
                    .map_err(|_| Error::Unsupported("invalid native Opus layout"))?;
                let decoder = MultistreamDecoder::new(rate, layout)
                    .map_err(|_| Error::DecodeError("failed to create native opus decoder"))?;
                Ok(Self::Native(Box::new(decoder), channels))
            }
            #[allow(unreachable_patterns)]
            _ => {
                let _ = (sample_rate, channels);
                Err(Error::Unsupported("requested opus backend is not enabled"))
            }
        }
    }

    fn reset(&mut self) -> Result<()> {
        match self {
            #[cfg(feature = "libopus")]
            Self::Libopus(decoder) => {
                use audiopus::coder::GenericCtl;
                decoder
                    .reset_state()
                    .map_err(|_| Error::DecodeError("libopus reset failed"))
            }
            #[cfg(feature = "native")]
            Self::Native(decoder, _) => decoder
                .reset_state()
                .map_err(|_| Error::DecodeError("native opus reset failed")),
            Self::Unavailable => Err(Error::DecodeError("opus decoder unavailable after reset")),
        }
    }

    fn decode(&mut self, packet: &[u8], output: &mut [i16]) -> Result<usize> {
        match self {
            #[cfg(feature = "libopus")]
            Self::Libopus(decoder) => decoder
                .decode(Some(packet), output, false)
                .map_err(|_| Error::DecodeError("libopus decode failed")),
            #[cfg(feature = "native")]
            Self::Native(decoder, channels) => {
                let capacity = output.len() / *channels;
                decoder
                    .decode(Some(packet), output, capacity, false)
                    .map_err(|error| match error {
                        moosicbox_opus_native::Error::Unsupported(_)
                        | moosicbox_opus_native::Error::UnsupportedMode(_) => {
                            Error::Unsupported("native opus configuration is not supported")
                        }
                        _ => Error::DecodeError("native opus decode failed"),
                    })
            }
            Self::Unavailable => {
                let _ = (packet, output);
                Err(Error::DecodeError("opus decoder unavailable after reset"))
            }
        }
    }
}

/// Opus decoder for Symphonia, with explicit reference/native backend selection.
///
/// Supports native mapping-family-one surround (up to eight channels) and mono/
/// stereo at 8, 12, 16, 24, and 48 kHz. With `libopus`
/// enabled, [`Self::try_new`] uses libopus even when `native` is also enabled.
/// A native-only build uses native. Use [`Self::try_new_with_backend`] for an
/// explicit comparison route. Native codec accuracy is still under development.
pub struct OpusDecoder {
    params: AudioCodecParameters,
    backend: OpusBackend,
    decoder: Mutex<BackendDecoder>,
    output_buf: AudioBuffer<f32>,
    temp_decode_buf: Vec<i16>,
    channel_count: usize,
    channel_order: Vec<usize>,
    output_gain: f32,
    gapless: bool,
}

/// Return packet channel indices in Symphonia's canonical speaker order.
fn opus_channel_order(
    head: &moosicbox_ogg_opus::OpusHead,
    channels: &symphonia::core::audio::Channels,
) -> Result<Vec<usize>> {
    use symphonia::core::audio::Position as P;

    let expected = crate::format::opus_channels(head)?;
    // Symphonia's stock Ogg reader labels mono as FRONT_LEFT for mapping
    // families 0 and 1, while our reader uses the canonical FRONT_CENTER.
    // Both describe the same single Opus channel; preserve the caller's label
    // without accepting arbitrary single-speaker or multichannel layouts.
    if head.channels == 1 && *channels == P::FRONT_LEFT.into() {
        return Ok(vec![0]);
    }
    if *channels != expected {
        return Err(Error::Unsupported(
            "Opus channel positions do not match header",
        ));
    }
    let speakers: &[P] = match head.channels {
        1 => &[P::FRONT_CENTER],
        2 => &[P::FRONT_LEFT, P::FRONT_RIGHT],
        3 => &[P::FRONT_LEFT, P::FRONT_CENTER, P::FRONT_RIGHT],
        4 => &[P::FRONT_LEFT, P::FRONT_RIGHT, P::REAR_LEFT, P::REAR_RIGHT],
        5 => &[
            P::FRONT_LEFT,
            P::FRONT_CENTER,
            P::FRONT_RIGHT,
            P::REAR_LEFT,
            P::REAR_RIGHT,
        ],
        6 => &[
            P::FRONT_LEFT,
            P::FRONT_CENTER,
            P::FRONT_RIGHT,
            P::REAR_LEFT,
            P::REAR_RIGHT,
            P::LFE1,
        ],
        7 => &[
            P::FRONT_LEFT,
            P::FRONT_CENTER,
            P::FRONT_RIGHT,
            P::SIDE_LEFT,
            P::SIDE_RIGHT,
            P::REAR_CENTER,
            P::LFE1,
        ],
        8 => &[
            P::FRONT_LEFT,
            P::FRONT_CENTER,
            P::FRONT_RIGHT,
            P::SIDE_LEFT,
            P::SIDE_RIGHT,
            P::REAR_LEFT,
            P::REAR_RIGHT,
            P::LFE1,
        ],
        _ => return Err(Error::Unsupported("unsupported Opus speaker mapping")),
    };
    let mut order = vec![0; speakers.len()];
    for (source, &speaker) in speakers.iter().enumerate() {
        let target = channels
            .get_canonical_index_for_positioned_channel(speaker)
            .ok_or(Error::Unsupported(
                "unsupported Opus channel representation",
            ))?;
        order[target] = source;
    }
    Ok(order)
}

impl OpusDecoder {
    /// Create a decoder using the feature-selected default backend.
    ///
    /// # Errors
    /// * Returns the same initialization errors as `try_new_with_backend`.
    pub fn try_new(params: &AudioCodecParameters, options: &AudioDecoderOptions) -> Result<Self> {
        let backend = if cfg!(feature = "libopus") {
            OpusBackend::Libopus
        } else {
            OpusBackend::Native
        };
        Self::try_new_with_backend_options(params, backend, options)
    }

    /// Create exactly the selected backend with the supplied Symphonia options.
    ///
    /// # Errors
    /// * Returns initialization errors from [`Self::try_new_with_backend`], including an unavailable backend.
    pub fn try_new_with_backend_options(
        params: &AudioCodecParameters,
        backend: OpusBackend,
        options: &AudioDecoderOptions,
    ) -> Result<Self> {
        let mut decoder = Self::try_new_with_backend(params, backend)?;
        decoder.gapless = options.gapless;
        Ok(decoder)
    }

    /// Create a decoder using exactly the requested implementation.
    ///
    /// # Errors
    ///
    /// * The requested backend feature is disabled.
    /// * The sample rate or channel count is unsupported.
    /// * Supplied `OpusHead` initialization data is malformed, channel positions
    ///   disagree, or mapping family is unsupported. Mandatory output gain is
    ///   applied after normalized PCM conversion, exactly once, preserving the
    ///   public float-output precision contract. Codec Q8 gain remains zero.
    ///   The format reader supplies pre-skip as packet trimming.
    /// * The libopus adapter is selected for a multistream configuration.
    /// * Backend initialization fails.
    pub fn try_new_with_backend(
        params: &AudioCodecParameters,
        backend: OpusBackend,
    ) -> Result<Self> {
        let sample_rate = params.sample_rate.unwrap_or(48000);
        if !matches!(sample_rate, 8000 | 12000 | 16000 | 24000 | 48000) {
            return Err(Error::Unsupported("unsupported sample rate"));
        }
        let channels = params
            .channels
            .clone()
            .unwrap_or_else(|| (Position::FRONT_LEFT | Position::FRONT_RIGHT).into());
        let channel_count = channels.count();
        if !(1..=8).contains(&channel_count) {
            return Err(Error::Unsupported("unsupported channel count"));
        }
        let head = params
            .extra_data
            .as_deref()
            .filter(|data| data.starts_with(b"OpusHead"))
            .map(moosicbox_ogg_opus::OpusHead::parse)
            .transpose()
            .map_err(|_| Error::DecodeError("invalid opus identification header"))?;
        let channel_order = if let Some(head) = &head {
            if usize::from(head.channels) != channel_count {
                return Err(Error::DecodeError("opus header channel count mismatch"));
            }
            opus_channel_order(head, &channels)?
        } else {
            if channel_count > 2 {
                return Err(Error::Unsupported(
                    "multichannel Opus requires identification header",
                ));
            }
            (0..channel_count).collect()
        };
        let decoder = BackendDecoder::new(backend, sample_rate, channel_count, head.as_ref())?;
        let output_gain = 10.0_f32
            .powf(f32::from(head.as_ref().map_or(0, |head| head.output_gain)) / (256.0 * 20.0));
        // RFC 6716 permits up to 120 ms per packet, independent of frame duration.
        let max_samples = sample_rate as usize * 120 / 1000;
        Ok(Self {
            params: params.clone(),
            backend,
            decoder: Mutex::new(decoder),
            output_buf: AudioBuffer::new(AudioSpec::new(sample_rate, channels), max_samples),
            temp_decode_buf: vec![0; max_samples * channel_count],
            channel_count,
            channel_order,
            output_gain,
            gapless: true,
        })
    }

    /// Return the selected backend (never an implicit fallback).
    #[must_use]
    pub const fn backend(&self) -> OpusBackend {
        self.backend
    }
}

const OPUS_CODEC: SupportedAudioCodec = SupportedAudioCodec {
    id: CODEC_ID_OPUS,
    info: CodecInfo {
        short_name: "opus",
        long_name: "Opus Interactive Audio Codec",
        profiles: &[],
    },
};

impl RegisterableAudioDecoder for OpusDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        options: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>> {
        Self::try_new(params, options).map(|decoder| Box::new(decoder) as Box<dyn AudioDecoder>)
    }
    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        &[OPUS_CODEC]
    }
}

impl AudioDecoder for OpusDecoder {
    fn codec_info(&self) -> &CodecInfo {
        &OPUS_CODEC.info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        self.output_buf.clear();
        if packet.data.is_empty() {
            return Err(Error::DecodeError("empty opus packet"));
        }
        // Both backends own packet framing, TOC, DTX, and multi-frame state.
        // Never strip headers or feed individual frame payloads to these APIs.
        let decoded_samples = self
            .decoder
            .get_mut()
            .map_err(|_| Error::DecodeError("opus decoder lock poisoned"))?
            .decode(packet.data, &mut self.temp_decode_buf)?;
        if decoded_samples > self.temp_decode_buf.len() / self.channel_count {
            return Err(Error::DecodeError("invalid decoded opus duration"));
        }
        self.output_buf.render_uninit(Some(decoded_samples));
        for ch in 0..self.channel_count {
            let output = self.output_buf.plane_mut(ch).unwrap();
            for (i, sample) in output.iter_mut().enumerate() {
                *sample = (f32::from(
                    self.temp_decode_buf[i * self.channel_count + self.channel_order[ch]],
                ) / 32768.0)
                    * self.output_gain;
            }
        }
        if self.gapless {
            self.output_buf.trim(
                usize::try_from(packet.trim_start.get()).unwrap_or(usize::MAX),
                usize::try_from(packet.trim_end.get()).unwrap_or(usize::MAX),
            );
        }
        Ok(self.output_buf.as_generic_audio_buffer_ref())
    }

    fn finalize(&mut self) -> FinalizeResult {
        // Opus has no delayed output to flush; packets do retain predictive state.
        FinalizeResult::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.output_buf.as_generic_audio_buffer_ref()
    }

    fn reset(&mut self) {
        let result = self
            .decoder
            .get_mut()
            .map_err(|_| Error::DecodeError("opus decoder lock poisoned"))
            .and_then(BackendDecoder::reset);
        if let Err(error) = result {
            warn!("Failed to reset opus decoder: {error}");
            self.decoder = Mutex::new(BackendDecoder::Unavailable);
        }
        self.output_buf.clear();
        self.temp_decode_buf.fill(0);
    }
}

#[cfg(test)]
mod channel_tests {
    use super::*;
    #[test]
    fn surround_plane_order_matches_canonical_speakers() {
        for (channels, expected) in [
            (3, vec![0, 2, 1]),
            (4, vec![0, 1, 2, 3]),
            (5, vec![0, 2, 1, 3, 4]),
            (6, vec![0, 2, 1, 5, 3, 4]),
            (7, vec![0, 2, 1, 6, 5, 3, 4]),
            (8, vec![0, 2, 1, 7, 5, 6, 3, 4]),
        ] {
            let mut head = moosicbox_ogg_opus::OpusHead::new(1, 0, 48_000).unwrap();
            head.mapping_family = 1;
            head.channels = channels;
            assert_eq!(
                opus_channel_order(&head, &crate::format::opus_channels(&head).unwrap()).unwrap(),
                expected
            );
        }
    }
}
