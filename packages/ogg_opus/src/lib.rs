//! Standalone, bounded-memory Ogg Opus framing. All sample positions use 48 kHz.
#![doc = include_str!("../README.md")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom, Write};

const MAX_PACKET: usize = 16 * 1024 * 1024;
const UNKNOWN_GRANULE: u64 = u64::MAX;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Ogg CRC-32 (non-reflected polynomial, zero initial value).
#[must_use]
pub fn ogg_crc(bytes: &[u8]) -> u32 {
    let mut crc = 0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    0x04c1_1db7
                } else {
                    0
                };
        }
    }
    crc
}

/// Identification header. Output gain is signed Q7.8 dB; applying it is the decoder's job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpusHead {
    /// Channel count (nonzero).
    pub channels: u8,
    /// Initial decoded samples to discard at 48 kHz.
    pub pre_skip: u16,
    /// Informational original input rate, not the Opus decode rate.
    pub input_sample_rate: u32,
    /// Signed Q7.8 dB output gain.
    pub output_gain: i16,
    /// Channel mapping family (0, 1, or 255).
    pub mapping_family: u8,
    /// Coded stream count (family zero infers one).
    pub stream_count: u8,
    /// Coupled stream count.
    pub coupled_count: u8,
    /// Output channel to coded channel map (empty for family zero).
    pub channel_mapping: Vec<u8>,
}

impl OpusHead {
    /// Construct a mono or stereo mapping-family-zero header.
    ///
    /// # Errors
    /// * Returns `InvalidInput` for a channel count other than one or two.
    pub fn new(channels: u8, pre_skip: u16, input_sample_rate: u32) -> io::Result<Self> {
        if !(1..=2).contains(&channels) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "family zero needs mono or stereo",
            ));
        }
        Ok(Self {
            channels,
            pre_skip,
            input_sample_rate,
            output_gain: 0,
            mapping_family: 0,
            stream_count: 1,
            coupled_count: channels - 1,
            channel_mapping: Vec::new(),
        })
    }

    /// Parse an identification packet, accepting compatible minor versions.
    ///
    /// # Errors
    /// * Rejects truncated headers, unsupported versions/families, and invalid channel maps.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 19 || &bytes[..8] != b"OpusHead" || bytes[8] == 0 || bytes[8] > 15 {
            return Err(invalid("invalid OpusHead"));
        }
        let channels = bytes[9];
        let family = bytes[18];
        if channels == 0 || ![0, 1, 255].contains(&family) {
            return Err(invalid("unsupported channel mapping"));
        }
        let (streams, coupled, mapping) = if family == 0 {
            if channels > 2 || (bytes[8] == 1 && bytes.len() != 19) {
                return Err(invalid("invalid family-zero header"));
            }
            (1, channels - 1, Vec::new())
        } else {
            let length = 21 + usize::from(channels);
            if bytes.len() < length || (bytes[8] == 1 && bytes.len() != length) {
                return Err(invalid("truncated channel mapping"));
            }
            let streams = bytes[19];
            let coupled = bytes[20];
            let coded = u16::from(streams) + u16::from(coupled);
            let mapping = bytes[21..length].to_vec();
            if streams == 0
                || coupled > streams
                || coded > 255
                || mapping.iter().any(|&x| x != 255 && u16::from(x) >= coded)
                || (family == 1 && channels > 8)
            {
                return Err(invalid("invalid channel mapping"));
            }
            (streams, coupled, mapping)
        };
        Ok(Self {
            channels,
            pre_skip: u16::from_le_bytes([bytes[10], bytes[11]]),
            input_sample_rate: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            output_gain: i16::from_le_bytes([bytes[16], bytes[17]]),
            mapping_family: family,
            stream_count: streams,
            coupled_count: coupled,
            channel_mapping: mapping,
        })
    }

    /// Serialize this identification header using `OpusHead` version one.
    ///
    /// # Errors
    /// * Rejects invalid channel counts, unsupported mapping families, and invalid maps.
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut bytes = b"OpusHead\x01".to_vec();
        bytes.push(self.channels);
        bytes.extend(self.pre_skip.to_le_bytes());
        bytes.extend(self.input_sample_rate.to_le_bytes());
        bytes.extend(self.output_gain.to_le_bytes());
        bytes.push(self.mapping_family);
        if self.mapping_family != 0 {
            bytes.extend([self.stream_count, self.coupled_count]);
            bytes.extend(&self.channel_mapping);
        }
        Self::parse(&bytes)?;
        Ok(bytes)
    }
}

/// Opus comments, retained as bytes because comment fields need not be valid UTF-8.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpusTags {
    /// Encoder/vendor identification.
    pub vendor: Vec<u8>,
    /// Uninterpreted `NAME=value` comment fields.
    pub comments: Vec<Vec<u8>>,
}

impl OpusTags {
    /// Parse comments with bounded lengths and counts. Trailing padding is accepted.
    ///
    /// # Errors
    /// * Rejects missing magic, truncated fields, or metadata over the packet limit.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_PACKET || !bytes.starts_with(b"OpusTags") {
            return Err(invalid("invalid OpusTags"));
        }
        let mut rest = &bytes[8..];
        let vendor = take_field(&mut rest)?.to_vec();
        let count = take_u32(&mut rest)? as usize;
        if count > rest.len() / 4 {
            return Err(invalid("truncated comment list"));
        }
        let mut comments = Vec::with_capacity(count);
        for _ in 0..count {
            comments.push(take_field(&mut rest)?.to_vec());
        }
        Ok(Self { vendor, comments })
    }

    fn encode(&self) -> io::Result<Vec<u8>> {
        let mut bytes = b"OpusTags".to_vec();
        put_field(&mut bytes, &self.vendor)?;
        bytes.extend(
            u32::try_from(self.comments.len())
                .map_err(|_| invalid("too many comments"))?
                .to_le_bytes(),
        );
        for comment in &self.comments {
            put_field(&mut bytes, comment)?;
        }
        if bytes.len() > MAX_PACKET {
            return Err(invalid("metadata too large"));
        }
        Ok(bytes)
    }
}

fn take_u32(rest: &mut &[u8]) -> io::Result<u32> {
    if rest.len() < 4 {
        return Err(invalid("truncated length"));
    }
    let value = u32::from_le_bytes(rest[..4].try_into().unwrap());
    *rest = &rest[4..];
    Ok(value)
}
fn take_field<'a>(rest: &mut &'a [u8]) -> io::Result<&'a [u8]> {
    let length = take_u32(rest)? as usize;
    if length > rest.len() {
        return Err(invalid("truncated field"));
    }
    let (field, tail) = rest.split_at(length);
    *rest = tail;
    Ok(field)
}
fn put_field(bytes: &mut Vec<u8>, value: &[u8]) -> io::Result<()> {
    if value.len() > MAX_PACKET.saturating_sub(bytes.len()).saturating_sub(4) {
        return Err(invalid("metadata exceeds packet limit"));
    }
    bytes.extend(
        u32::try_from(value.len())
            .map_err(|_| invalid("field too large"))?
            .to_le_bytes(),
    );
    bytes.extend(value);
    Ok(())
}

/// Return the decoded duration of an Opus packet at 48 kHz (not entropy validation).
///
/// # Errors
/// * Rejects empty packets, missing frame count, zero frames, and durations above 120 ms.
pub fn packet_duration(packet: &[u8]) -> io::Result<u32> {
    let &toc = packet.first().ok_or_else(|| invalid("empty Opus packet"))?;
    let config = toc >> 3;
    let frame = if config >= 16 {
        120 << (config & 3)
    } else if config >= 12 {
        480 << (config & 1)
    } else {
        [480, 960, 1920, 2880][usize::from(config & 3)]
    };
    let count = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u32::from(
            *packet
                .get(1)
                .ok_or_else(|| invalid("missing frame count"))?
                & 63,
        ),
    };
    let duration = frame * count;
    if count == 0 || duration > 5760 {
        return Err(invalid("invalid Opus duration"));
    }
    Ok(duration)
}

struct Page {
    flags: u8,
    granule: u64,
    serial: u32,
    sequence: u32,
    lacing: Vec<u8>,
    body: Vec<u8>,
}
impl Page {
    fn read(reader: &mut impl Read) -> io::Result<Option<Self>> {
        let mut header = [0_u8; 27];
        loop {
            match reader.read(&mut header[..1]) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        reader.read_exact(&mut header[1..])?;
        if &header[..4] != b"OggS" || header[4] != 0 || header[5] & !7 != 0 {
            return Err(invalid("invalid Ogg page header"));
        }
        let mut lacing = vec![0; usize::from(header[26])];
        reader.read_exact(&mut lacing)?;
        let mut body = vec![0; lacing.iter().map(|&n| usize::from(n)).sum()];
        reader.read_exact(&mut body)?;
        let checksum = u32::from_le_bytes(header[22..26].try_into().unwrap());
        header[22..26].fill(0);
        let mut bytes = header.to_vec();
        bytes.extend(&lacing);
        bytes.extend(&body);
        if ogg_crc(&bytes) != checksum {
            return Err(invalid("Ogg checksum mismatch"));
        }
        Ok(Some(Self {
            flags: header[5],
            granule: u64::from_le_bytes(header[6..14].try_into().unwrap()),
            serial: u32::from_le_bytes(header[14..18].try_into().unwrap()),
            sequence: u32::from_le_bytes(header[18..22].try_into().unwrap()),
            lacing,
            body,
        }))
    }
}

/// One audio packet and the exact samples to retain after decoding at 48 kHz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioPacket {
    /// Encoded bytes, unchanged from the source.
    pub data: Vec<u8>,
    /// Decoded packet start on the granule timeline, before pre-skip removal.
    pub decoded_start: u64,
    /// Full decoded duration per channel.
    pub duration: u32,
    /// Samples to discard from the decoded beginning, including pre-skip.
    pub trim_start: u32,
    /// Samples to discard from the decoded end (EOS padding).
    pub trim_end: u32,
    /// Whether this packet is the final packet of the logical stream.
    pub end_of_stream: bool,
}
impl AudioPacket {
    /// Number of audible samples per channel retained from this packet.
    #[must_use]
    pub const fn audible_samples(&self) -> u32 {
        self.duration
            .saturating_sub(self.trim_start)
            .saturating_sub(self.trim_end)
    }
}

/// Streaming, single-logical-stream reader. Multiplexing and chaining are rejected.
pub struct OggOpusReader<R> {
    source: R,
    head: OpusHead,
    tags: OpusTags,
    serial: Option<u32>,
    sequence: u32,
    partial: Vec<u8>,
    queue: VecDeque<AudioPacket>,
    headers: usize,
    position: u64,
    granule_origin: u64,
    first_audio: bool,
    eos: bool,
}

impl<R: Read> OggOpusReader<R> {
    /// Read and validate both Opus headers, leaving audio ready for streaming.
    ///
    /// # Errors
    /// * Propagates I/O errors and rejects corrupt pages, headers, or unsupported stream topology.
    pub fn new(source: R) -> io::Result<Self> {
        let mut reader = Self {
            source,
            head: OpusHead::new(1, 0, 48000)?,
            tags: OpusTags::default(),
            serial: None,
            sequence: 0,
            partial: Vec::new(),
            queue: VecDeque::new(),
            headers: 0,
            position: 0,
            granule_origin: 0,
            first_audio: true,
            eos: false,
        };
        while reader.headers < 2 {
            reader.read_page()?;
        }
        Ok(reader)
    }
    /// Identification metadata.
    #[must_use]
    pub const fn head(&self) -> &OpusHead {
        &self.head
    }
    /// Comment metadata.
    #[must_use]
    pub const fn tags(&self) -> &OpusTags {
        &self.tags
    }
    /// Absolute granule offset of decoded sample zero for this logical stream.
    ///
    /// Returns `None` until the first audio page has been read. After a successful
    /// `next_packet`, subtract this offset and [`OpusHead::pre_skip`] from the
    /// packet's `decoded_start` to obtain its signed audible-timeline timestamp.
    /// A seek resets this value until the first audio page is read again.
    pub const fn granule_origin(&self) -> Option<u64> {
        if self.first_audio {
            None
        } else {
            Some(self.granule_origin)
        }
    }

    /// Recover the underlying source at its current byte position.
    #[must_use]
    pub fn into_inner(self) -> R {
        self.source
    }

    /// Read the next encoded audio packet with gapless trimming metadata.
    ///
    /// # Errors
    /// * Returns I/O errors, invalid framing/CRC/sequence/granules, or missing EOS errors.
    pub fn next_packet(&mut self) -> io::Result<Option<AudioPacket>> {
        while self.queue.is_empty() && !self.eos {
            self.read_page()?;
        }
        Ok(self.queue.pop_front())
    }

    #[allow(clippy::too_many_lines)] // Keep page validation and state publication together.
    fn read_page(&mut self) -> io::Result<()> {
        let page =
            Page::read(&mut self.source)?.ok_or_else(|| invalid("stream ended before EOS"))?;
        if self.serial.is_none() {
            if page.flags != 2 || page.sequence != 0 {
                return Err(invalid("missing initial BOS"));
            }
            self.serial = Some(page.serial);
        } else if page.flags & 2 != 0 {
            return Err(invalid("unexpected BOS"));
        }
        if self.serial != Some(page.serial) || page.sequence != self.sequence {
            return Err(invalid("Ogg serial or sequence mismatch"));
        }
        self.sequence = self.sequence.wrapping_add(1);
        if (page.flags & 1 != 0) == self.partial.is_empty() {
            return Err(invalid("invalid packet continuation"));
        }
        let headers_before = self.headers;
        let mut audio = Vec::new();
        let mut offset = 0;
        let mut completed = 0;
        for &lace in &page.lacing {
            let size = usize::from(lace);
            if self.partial.len() + size > MAX_PACKET {
                return Err(invalid("packet exceeds limit"));
            }
            self.partial
                .extend_from_slice(&page.body[offset..offset + size]);
            offset += size;
            if lace < 255 {
                completed += 1;
                let bytes = std::mem::take(&mut self.partial);
                match self.headers {
                    0 => {
                        self.head = OpusHead::parse(&bytes)?;
                        self.headers = 1;
                    }
                    1 => {
                        self.tags = OpusTags::parse(&bytes)?;
                        self.headers = 2;
                    }
                    _ => {
                        let duration = packet_duration(&bytes)?;
                        audio.push((bytes, duration));
                    }
                }
            }
        }
        if headers_before == 0 && (completed != 1 || !self.partial.is_empty() || page.granule != 0)
        {
            return Err(invalid("OpusHead must occupy its own page"));
        }
        if headers_before < 2
            && (!audio.is_empty() || (self.headers == 2 && !self.partial.is_empty()))
        {
            return Err(invalid("audio shares header page"));
        }
        let eos = page.flags & 4 != 0;
        if eos && (self.headers < 2 || !self.partial.is_empty() || audio.is_empty()) {
            return Err(invalid("incomplete EOS page"));
        }
        if audio.is_empty() {
            if (completed == 0 && page.granule != UNKNOWN_GRANULE)
                || (completed > 0 && page.granule != 0)
            {
                return Err(invalid("invalid header/continuation granule"));
            }
            return Ok(());
        }
        if page.granule == UNKNOWN_GRANULE {
            return Err(invalid("missing audio granule"));
        }
        let total: u64 = audio.iter().map(|(_, duration)| u64::from(*duration)).sum();
        if self.first_audio {
            self.position = page.granule.saturating_sub(total);
            self.granule_origin = self.position;
            self.first_audio = false;
        }
        let end = self
            .position
            .checked_add(total)
            .ok_or_else(|| invalid("sample position overflow"))?;
        if (!eos && page.granule != end)
            || (eos
                && (page.granule > end
                    || page.granule < self.position
                    || page.granule.saturating_sub(self.granule_origin)
                        < u64::from(self.head.pre_skip)))
        {
            return Err(invalid("invalid audio granule position"));
        }
        let count = audio.len();
        for (index, (data, duration)) in audio.into_iter().enumerate() {
            let trim_start = u32::try_from(
                u64::from(self.head.pre_skip)
                    .saturating_sub(self.position - self.granule_origin)
                    .min(u64::from(duration)),
            )
            .map_err(|_| invalid("trim overflow"))?;
            let packet_end = self.position + u64::from(duration);
            let trim_end = u32::try_from(
                packet_end
                    .saturating_sub(page.granule)
                    .min(u64::from(duration - trim_start)),
            )
            .map_err(|_| invalid("trim overflow"))?;
            self.queue.push_back(AudioPacket {
                data,
                decoded_start: self.position,
                duration,
                trim_start,
                trim_end,
                end_of_stream: eos && index + 1 == count,
            });
            self.position = packet_end;
        }
        self.eos = eos;
        Ok(())
    }
}

/// Seek result: reset the codec, decode from the returned preroll packet, then discard
/// all decoded samples before `discard_until` on the granule timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeekResult {
    /// Requested audible sample position, clamped to the end of the stream.
    pub sample: u64,
    /// Granule timeline cutoff (includes pre-skip).
    pub discard_until: u64,
}

impl<R: Read + Seek> OggOpusReader<R> {
    fn reset(&mut self) {
        self.serial = None;
        self.sequence = 0;
        self.partial.clear();
        self.queue.clear();
        self.headers = 0;
        self.position = 0;
        self.granule_origin = 0;
        self.first_audio = true;
        self.eos = false;
    }
    /// Return the audible duration in 48-kHz samples using a bounded-memory scan.
    /// Preserves both the current packet queue and the underlying byte position.
    ///
    /// # Errors
    /// * Propagates seek, I/O, or stream validation errors. If restoring the source
    ///   position fails, the reader must not be used further.
    pub fn total_samples(&mut self) -> io::Result<u64> {
        let offset = self.source.stream_position()?;
        let result = (|| {
            self.source.seek(SeekFrom::Start(0))?;
            let mut scan = OggOpusReader::new(&mut self.source)?;
            let mut end = 0;
            while let Some(packet) = scan.next_packet()? {
                if packet.end_of_stream {
                    end = packet.decoded_start + u64::from(packet.duration - packet.trim_end);
                }
            }
            Ok(end
                .saturating_sub(scan.granule_origin)
                .saturating_sub(u64::from(scan.head.pre_skip)))
        })();
        self.source.seek(SeekFrom::Start(offset))?;
        result
    }

    /// Seek to an audible 48-kHz sample, retaining at least 80 ms of available preroll.
    /// Uses a bounded-memory linear scan, not an unbounded packet index. Sources must
    /// start at byte zero. The caller must reset its decoder before consuming packets.
    ///
    /// # Errors
    /// * Propagates seek, read, and stream validation errors, including sample overflow.
    pub fn seek_to(&mut self, sample: u64) -> io::Result<SeekResult> {
        // First determine the actual EOS position, including end trimming. This
        // also validates the stream before exposing a seek result beyond its end.
        self.source.seek(SeekFrom::Start(0))?;
        self.reset();
        let mut end = 0;
        while let Some(packet) = self.next_packet()? {
            if packet.end_of_stream {
                end = packet.decoded_start + u64::from(packet.duration - packet.trim_end);
            }
        }
        let sample = sample.min(
            end.saturating_sub(self.granule_origin)
                .saturating_sub(u64::from(self.head.pre_skip)),
        );
        let target = sample
            .checked_add(self.granule_origin)
            .and_then(|value| value.checked_add(u64::from(self.head.pre_skip)))
            .ok_or_else(|| invalid("seek overflow"))?;
        let preroll = target.saturating_sub(3840);
        self.source.seek(SeekFrom::Start(0))?;
        self.reset();
        while self.headers < 2 {
            self.read_page()?;
        }
        loop {
            let Some(packet) = self.next_packet()? else {
                let end = self.position.saturating_sub(u64::from(self.head.pre_skip));
                return Ok(SeekResult {
                    sample: sample.min(end),
                    discard_until: target.min(self.position),
                });
            };
            if packet.decoded_start + u64::from(packet.duration) > preroll {
                self.queue.push_front(packet);
                return Ok(SeekResult {
                    sample,
                    discard_until: target,
                });
            }
        }
    }
}

/// Streaming Ogg Opus writer. Buffers one audio packet so EOS can carry exact trim.
pub struct OggOpusWriter<W> {
    sink: W,
    head: OpusHead,
    serial: u32,
    sequence: u32,
    decoded: u64,
    pending: Option<(Vec<u8>, u32)>,
}
impl<W: Write> OggOpusWriter<W> {
    /// Write identification and comment headers with the supplied logical-stream serial.
    ///
    /// # Errors
    /// * Rejects invalid headers/oversized tags and propagates write errors.
    pub fn new(sink: W, head: OpusHead, tags: &OpusTags, serial: u32) -> io::Result<Self> {
        let identification = head.encode()?;
        let comments = tags.encode()?;
        let mut writer = Self {
            sink,
            head,
            serial,
            sequence: 0,
            decoded: 0,
            pending: None,
        };
        writer.packet(&identification, 0, 2)?;
        writer.packet(&comments, 0, 0)?;
        Ok(writer)
    }
    /// Queue one complete Opus packet. Duration is derived from its TOC, not supplied.
    ///
    /// # Errors
    /// * Rejects invalid duration or oversized packets and propagates I/O/overflow errors.
    pub fn write_packet(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_PACKET {
            return Err(invalid("packet exceeds limit"));
        }
        let duration = packet_duration(bytes)?;
        if let Some((previous, samples)) = self.pending.take() {
            self.decoded = self
                .decoded
                .checked_add(u64::from(samples))
                .ok_or_else(|| invalid("sample overflow"))?;
            self.packet(&previous, self.decoded, 0)?;
        }
        self.pending = Some((bytes.to_vec(), duration));
        Ok(())
    }
    /// Finish with the actual audible sample count (per channel, 48 kHz). Encoder
    /// lookahead belongs in `OpusHead::pre_skip`; callers must supply enough encoded
    /// padding to cover it. Only the final packet may be shortened by EOS.
    ///
    /// # Errors
    /// * Rejects missing packets, overflow, or a count outside the final packet.
    /// * Propagates write and flush errors.
    pub fn finish(mut self, audible_samples: u64) -> io::Result<W> {
        let (packet, duration) = self
            .pending
            .take()
            .ok_or_else(|| invalid("no audio packets"))?;
        let granule = audible_samples
            .checked_add(u64::from(self.head.pre_skip))
            .ok_or_else(|| invalid("sample overflow"))?;
        let end = self
            .decoded
            .checked_add(u64::from(duration))
            .ok_or_else(|| invalid("sample overflow"))?;
        if granule < self.decoded || granule > end {
            return Err(invalid("final sample count outside final packet"));
        }
        self.packet(&packet, granule, 4)?;
        self.sink.flush()?;
        Ok(self.sink)
    }
    fn packet(&mut self, bytes: &[u8], granule: u64, flags: u8) -> io::Result<()> {
        let mut laces = vec![255_u8; bytes.len() / 255];
        laces.push(u8::try_from(bytes.len() % 255).map_err(|_| invalid("invalid lace"))?);
        let mut offset = 0;
        let pages = laces.len().div_ceil(255);
        for (index, lacing) in laces.chunks(255).enumerate() {
            let last = index + 1 == pages;
            let size: usize = lacing.iter().map(|&x| usize::from(x)).sum();
            let page_flags =
                if index == 0 { flags & 2 } else { 1 } | if last { flags & 4 } else { 0 };
            let mut page = b"OggS\0".to_vec();
            page.push(page_flags);
            page.extend(if last { granule } else { UNKNOWN_GRANULE }.to_le_bytes());
            page.extend(self.serial.to_le_bytes());
            page.extend(self.sequence.to_le_bytes());
            page.extend([0; 4]);
            page.push(u8::try_from(lacing.len()).map_err(|_| invalid("too many laces"))?);
            page.extend(lacing);
            page.extend(&bytes[offset..offset + size]);
            let crc = ogg_crc(&page);
            page[22..26].copy_from_slice(&crc.to_le_bytes());
            self.sink.write_all(&page)?;
            self.sequence = self.sequence.wrapping_add(1);
            offset += size;
        }
        Ok(())
    }
}
