//! Container-independent Opus multistream channel mapping and self-delimited framing.
//!
//! Coupled streams precede mono streams. Mapping entries address decoded channels,
//! not speaker positions; 255 denotes silence. No speaker layout is inferred.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use crate::{Channels, Decoder, Error, Result, SampleRate, framing, toc::Toc};

fn invalid(message: &str) -> Error {
    Error::InvalidPacket(message.into())
}

/// Validated output-channel to coded-channel mapping, independent of Ogg families.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultistreamLayout {
    streams: u8,
    coupled: u8,
    mapping: Vec<u8>,
}

impl MultistreamLayout {
    /// Construct an explicit mapping. Duplicate entries and silent outputs are legal.
    ///
    /// # Errors
    /// * Rejects zero or more than 255 outputs, zero streams, excessive coupling,
    ///   more than 255 coded channels, or out-of-range non-silent mapping entries.
    pub fn new(streams: u8, coupled: u8, mapping: Vec<u8>) -> Result<Self> {
        let coded = u16::from(streams) + u16::from(coupled);
        if streams == 0
            || coupled > streams
            || coded > 255
            || mapping.is_empty()
            || mapping.len() > 255
            || mapping.iter().any(|&x| x != 255 && u16::from(x) >= coded)
        {
            return Err(Error::Unsupported(
                "invalid multistream channel mapping".into(),
            ));
        }
        Ok(Self {
            streams,
            coupled,
            mapping,
        })
    }

    /// Number of interleaved application channels.
    #[must_use]
    pub const fn channels(&self) -> usize {
        self.mapping.len()
    }

    /// Number of elementary Opus streams.
    #[must_use]
    pub const fn streams(&self) -> u8 {
        self.streams
    }

    /// Number of leading stereo streams.
    #[must_use]
    pub const fn coupled_streams(&self) -> u8 {
        self.coupled
    }

    /// Output-channel to coded-channel map; 255 means silence.
    #[must_use]
    pub fn mapping(&self) -> &[u8] {
        &self.mapping
    }

    fn stream_channels(&self, stream: usize) -> Channels {
        if stream < usize::from(self.coupled) {
            Channels::Stereo
        } else {
            Channels::Mono
        }
    }

    #[cfg(feature = "celt")]
    fn coded_start(&self, stream: usize) -> usize {
        stream + stream.min(usize::from(self.coupled))
    }
}

fn read_byte(data: &[u8], cursor: &mut usize) -> Result<u8> {
    let byte = *data
        .get(*cursor)
        .ok_or_else(|| invalid("truncated multistream header"))?;
    *cursor += 1;
    Ok(byte)
}

fn read_size(data: &[u8], cursor: &mut usize) -> Result<usize> {
    let first = usize::from(read_byte(data, cursor)?);
    Ok(if first < 252 {
        first
    } else {
        first + 4 * usize::from(read_byte(data, cursor)?)
    })
}

fn write_size(size: usize, output: &mut Vec<u8>) -> Result<()> {
    if size > 1275 {
        return Err(invalid("Opus frame exceeds 1275 bytes"));
    }
    if size < 252 {
        output.push(u8::try_from(size).map_err(|_| invalid("invalid frame length"))?);
    } else {
        let first = 252 + (size & 3);
        output.push(u8::try_from(first).map_err(|_| invalid("invalid frame length"))?);
        output.push(u8::try_from((size - first) / 4).map_err(|_| invalid("invalid frame length"))?);
    }
    Ok(())
}

// Canonical code-3 VBR packet. Self-delimited form explicitly sizes the last frame.
fn repacketize(toc: u8, frames: &[&[u8]], delimited: bool) -> Result<Vec<u8>> {
    let count = u8::try_from(frames.len()).map_err(|_| invalid("too many frames"))?;
    if count == 0 || count > 48 {
        return Err(invalid("invalid frame count"));
    }
    let mut packet = vec![(toc & 0xfc) | 3, 0x80 | count];
    for (index, frame) in frames.iter().enumerate() {
        if frame.len() > 1275 {
            return Err(invalid("Opus frame exceeds 1275 bytes"));
        }
        if delimited || index + 1 < frames.len() {
            write_size(frame.len(), &mut packet)?;
        }
    }
    for frame in frames {
        packet.extend_from_slice(frame);
    }
    Ok(packet)
}

fn self_delimited(data: &[u8]) -> Result<(Vec<u8>, usize)> {
    let mut cursor = 0;
    let toc = read_byte(data, &mut cursor)?;
    let mut padding = 0_usize;
    let (count, vbr) = match toc & 3 {
        0 => (1, false),
        1 => (2, false),
        2 => (2, true),
        _ => {
            let control = read_byte(data, &mut cursor)?;
            let count = usize::from(control & 63);
            if count == 0 || count > 48 {
                return Err(invalid("invalid multistream frame count"));
            }
            if control & 64 != 0 {
                loop {
                    let byte = read_byte(data, &mut cursor)?;
                    padding = padding
                        .checked_add(if byte == 255 { 254 } else { usize::from(byte) })
                        .ok_or_else(|| invalid("padding overflow"))?;
                    if padding > data.len() {
                        return Err(invalid("truncated padding"));
                    }
                    if byte != 255 {
                        break;
                    }
                }
            }
            (count, control & 128 != 0)
        }
    };
    let mut sizes = Vec::with_capacity(count);
    if vbr {
        for _ in 1..count {
            sizes.push(read_size(data, &mut cursor)?);
        }
    }
    let last = read_size(data, &mut cursor)?;
    if vbr {
        sizes.push(last);
    } else {
        sizes.resize(count, last);
    }
    let payload_size: usize = sizes.iter().sum();
    let end = cursor
        .checked_add(payload_size)
        .and_then(|x| x.checked_add(padding))
        .filter(|&x| x <= data.len())
        .ok_or_else(|| invalid("truncated self-delimited packet"))?;
    let mut frames = Vec::with_capacity(count);
    for size in sizes {
        frames.push(&data[cursor..cursor + size]);
        cursor += size;
    }
    Ok((repacketize(toc, &frames, false)?, end))
}

fn duration(packet: &[u8], rate: SampleRate) -> Result<usize> {
    let frames = framing::parse_frames(packet)?;
    if frames.iter().any(|x| x.len() > 1275) {
        return Err(invalid("oversized Opus frame"));
    }
    let tenths = usize::from(Toc::parse(packet[0]).frame_duration_tenths_ms()) * frames.len();
    if tenths == 0 || tenths > 1200 {
        return Err(invalid("invalid Opus packet duration"));
    }
    Ok(tenths * rate as usize / 10_000)
}

/// Split an RFC 6716 Appendix B multistream packet into ordinary Opus packets.
/// All but the final stream use self-delimited framing. Padding is discarded.
///
/// # Errors
/// * Rejects zero streams, truncated or invalid framing, oversized frames,
///   durations beyond 120 ms, or streams with unequal durations.
pub fn split_packet(packet: &[u8], streams: u8) -> Result<Vec<Vec<u8>>> {
    if streams == 0 {
        return Err(invalid("zero multistream streams"));
    }
    let mut offset = 0;
    let mut packets = Vec::with_capacity(usize::from(streams));
    for stream in 0..streams {
        if stream + 1 == streams {
            packets.push(packet[offset..].to_vec());
        } else {
            let (elementary, consumed) = self_delimited(&packet[offset..])?;
            offset += consumed;
            packets.push(elementary);
        }
    }
    let expected = duration(&packets[0], SampleRate::Hz48000)?;
    for elementary in &packets[1..] {
        if duration(elementary, SampleRate::Hz48000)? != expected {
            return Err(invalid("multistream durations differ"));
        }
    }
    Ok(packets)
}

/// Join equally long ordinary Opus packets into a self-delimited multistream packet.
///
/// # Errors
/// * Rejects empty or more than 255 streams, malformed packets, oversized frames,
///   invalid duration, or unequal stream durations.
pub fn join_packets(packets: &[Vec<u8>]) -> Result<Vec<u8>> {
    if packets.is_empty() || packets.len() > 255 {
        return Err(invalid("invalid stream count"));
    }
    let expected = duration(&packets[0], SampleRate::Hz48000)?;
    let mut joined = Vec::new();
    for (index, packet) in packets.iter().enumerate() {
        if duration(packet, SampleRate::Hz48000)? != expected {
            return Err(invalid("multistream durations differ"));
        }
        joined.extend(repacketize(
            packet[0],
            &framing::parse_frames(packet)?,
            index + 1 < packets.len(),
        )?);
    }
    Ok(joined)
}

/// Multistream decoder composing independent native mono/stereo decoders.
///
/// Packet structure and capacity are checked before any elementary decoder runs.
/// A codec error can advance earlier streams; reset before recovering from it.
pub struct MultistreamDecoder {
    layout: MultistreamLayout,
    rate: SampleRate,
    decoders: Vec<Decoder>,
    gain: i16,
}

impl MultistreamDecoder {
    /// Create one native decoder per stream.
    ///
    /// # Errors
    /// * Propagates elementary decoder initialization failures.
    pub fn new(rate: SampleRate, layout: MultistreamLayout) -> Result<Self> {
        let decoders = (0..usize::from(layout.streams))
            .map(|s| Decoder::new(rate, layout.stream_channels(s)))
            .collect::<Result<_>>()?;
        Ok(Self {
            layout,
            rate,
            decoders,
            gain: 0,
        })
    }

    /// Set signed Q7.8 dB gain. Gain is preserved across state resets.
    pub fn set_gain(&mut self, gain: i16) {
        self.gain = gain;
        for decoder in &mut self.decoders {
            decoder.set_gain(gain);
        }
    }

    /// Current signed Q7.8 dB gain.
    #[must_use]
    pub const fn gain(&self) -> i16 {
        self.gain
    }

    /// Decode interleaved PCM. `samples` is capacity per channel, and the exact
    /// requested duration for PLC/FEC. Empty packets are errors; use `None` for loss.
    ///
    /// # Errors
    /// * Rejects malformed/mismatched streams, invalid capacity or loss duration.
    /// * Propagates elementary decode/FEC errors and unsupported codec modes.
    pub fn decode(
        &mut self,
        packet: Option<&[u8]>,
        output: &mut [i16],
        samples: usize,
        fec: bool,
    ) -> Result<usize> {
        if samples == 0
            || samples > self.rate as usize * 120 / 1000
            || samples > output.len() / self.layout.channels()
        {
            return Err(invalid("invalid multistream output capacity"));
        }
        let packets = packet
            .map(|p| split_packet(p, self.layout.streams))
            .transpose()?;
        let actual = if let Some(packets) = &packets {
            let n = duration(&packets[0], self.rate)?;
            if !fec && n > samples {
                return Err(invalid("multistream output too short"));
            }
            if fec { samples } else { n }
        } else {
            samples
        };
        if (packets.is_none() || fec) && !samples.is_multiple_of(self.rate as usize / 400) {
            return Err(invalid("invalid multistream concealment duration"));
        }
        let mut decoded = Vec::with_capacity(self.decoders.len());
        for (stream, decoder) in self.decoders.iter_mut().enumerate() {
            let channels = self.layout.stream_channels(stream) as usize;
            let mut pcm = vec![0; actual * channels];
            let count = if let Some(packets) = &packets {
                decoder.decode(Some(&packets[stream]), &mut pcm, fec)?
            } else {
                decoder.decode_loss(&mut pcm, actual)?
            };
            if count != actual {
                return Err(invalid("elementary decoder duration mismatch"));
            }
            decoded.push(pcm);
        }
        for (channel, &mapped) in self.layout.mapping.iter().enumerate() {
            let coded = usize::from(mapped);
            let (stream, side) = if coded < 2 * usize::from(self.layout.coupled) {
                (coded / 2, coded % 2)
            } else {
                (coded - usize::from(self.layout.coupled), 0)
            };
            for sample in 0..actual {
                output[sample * self.layout.channels() + channel] = if mapped == 255 {
                    0
                } else {
                    let width = self.layout.stream_channels(stream) as usize;
                    decoded[stream][sample * width + side]
                };
            }
        }
        Ok(actual)
    }

    /// Conceal loss for the exact duration requested.
    ///
    /// # Errors
    /// * Rejects invalid duration/capacity and propagates codec concealment errors.
    pub fn decode_loss(&mut self, output: &mut [i16], samples: usize) -> Result<usize> {
        self.decode(None, output, samples, false)
    }

    /// Reset every elementary decoder, preserving channel layout and gain.
    ///
    /// # Errors
    /// * Propagates elementary decoder reset failures.
    pub fn reset_state(&mut self) -> Result<()> {
        for decoder in &mut self.decoders {
            decoder.reset_state()?;
        }
        Ok(())
    }
}

/// Multistream encoder configuration uses one ordinary encoder config per stream.
///
/// Every coded channel must be present in the input map. Duplicate mappings use
/// the first input channel, as in the reference generic multistream encoder.
#[cfg(feature = "celt")]
#[derive(Clone)]
pub struct MultistreamEncoder {
    layout: MultistreamLayout,
    configs: Vec<crate::encoder::EncoderConfig>,
    encoders: Vec<crate::encoder::Encoder>,
    input_channels: Vec<usize>,
}

#[cfg(feature = "celt")]
impl MultistreamEncoder {
    /// Construct independent native encoders with equal rates and frame durations.
    /// Bitrates are per stream, not a surround allocation policy.
    ///
    /// # Errors
    /// * Rejects mismatched configs or elementary lookaheads, missing coded input
    ///   channels, or unsupported elementary encoder configuration. No surround
    ///   analysis is claimed.
    pub fn new(
        layout: MultistreamLayout,
        configs: Vec<crate::encoder::EncoderConfig>,
    ) -> Result<Self> {
        if configs.len() != usize::from(layout.streams) {
            return Err(invalid("encoder stream count mismatch"));
        }
        for (stream, config) in configs.iter().enumerate() {
            if config.channels != layout.stream_channels(stream)
                || config.sample_rate != configs[0].sample_rate
                || config.frame_size != configs[0].frame_size
            {
                return Err(invalid("encoder stream configuration mismatch"));
            }
        }
        let input_channels = (0..usize::from(layout.streams) + usize::from(layout.coupled))
            .map(|coded| {
                layout
                    .mapping
                    .iter()
                    .position(|&x| usize::from(x) == coded)
                    .ok_or_else(|| invalid("encoder coded channel missing from mapping"))
            })
            .collect::<Result<Vec<_>>>()?;
        let encoders = configs
            .iter()
            .copied()
            .map(crate::encoder::Encoder::new)
            .collect::<Result<Vec<_>>>()?;
        if encoders
            .iter()
            .any(|encoder| encoder.lookahead() != encoders[0].lookahead())
        {
            return Err(invalid("multistream encoder lookaheads differ"));
        }
        Ok(Self {
            layout,
            configs,
            encoders,
            input_channels,
        })
    }

    /// Shared elementary encoder delay in input-rate samples per channel.
    /// Construction rejects unequal stream delays so a single container pre-skip
    /// can represent every output channel. The delay is unchanged by reset/flush.
    #[must_use]
    pub fn lookahead(&self) -> usize {
        self.encoders[0].lookahead()
    }

    /// Encode one interleaved input frame. All stream states commit together.
    ///
    /// # Errors
    /// * Rejects wrong PCM size, nonfinite/out-of-range samples, or propagates
    ///   elementary encoder/framing failures. State is unchanged on error.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        if pcm.len() != self.configs[0].frame_size * self.layout.channels()
            || pcm.iter().any(|x| !x.is_finite() || x.abs() > 1.0)
        {
            return Err(invalid("invalid multistream encoder PCM"));
        }
        let mut next = self.clone();
        let mut packets = Vec::with_capacity(next.encoders.len());
        for (stream, encoder) in next.encoders.iter_mut().enumerate() {
            let width = next.layout.stream_channels(stream) as usize;
            let start = next.layout.coded_start(stream);
            let mut input = Vec::with_capacity(next.configs[0].frame_size * width);
            for frame in pcm.chunks_exact(next.layout.channels()) {
                for side in 0..width {
                    input.push(frame[next.input_channels[start + side]]);
                }
            }
            packets.push(encoder.encode(&input)?);
        }
        let packet = join_packets(&packets)?;
        *self = next;
        Ok(packet)
    }

    /// Flush every stream's analysis overlap, preserving multistream framing.
    /// Container owners must trim the padding using the original input duration.
    ///
    /// # Errors
    /// * Propagates elementary flush/framing errors or inconsistent stream flush
    ///   counts. All stream states remain unchanged on error.
    pub fn finish(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut next = self.clone();
        let flushed = next
            .encoders
            .iter_mut()
            .map(crate::encoder::Encoder::finish)
            .collect::<Result<Vec<_>>>()?;
        let count = flushed[0].len();
        if flushed.iter().any(|stream| stream.len() != count) {
            return Err(invalid("multistream flush packet counts differ"));
        }
        let mut packets = Vec::with_capacity(count);
        for index in 0..count {
            let elementary = flushed
                .iter()
                .map(|stream| stream[index].clone())
                .collect::<Vec<_>>();
            packets.push(join_packets(&elementary)?);
        }
        *self = next;
        Ok(packets)
    }

    /// Recreate all elementary encoder states with the original configuration.
    ///
    /// # Errors
    /// * Propagates elementary encoder initialization errors; state is unchanged.
    pub fn reset_state(&mut self) -> Result<()> {
        let encoders = self
            .configs
            .iter()
            .copied()
            .map(crate::encoder::Encoder::new)
            .collect::<Result<_>>()?;
        self.encoders = encoders;
        Ok(())
    }
}
