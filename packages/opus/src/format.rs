//! Symphonia adapter for the standalone Ogg Opus container reader.

use moosicbox_ogg_opus::{ChainEvent, ChainSection, OggOpusChainReader};
use std::{
    io::{self, BufReader, Read, Seek, SeekFrom},
    num::NonZeroU32,
};
use symphonia::core::{
    audio::{
        Channels,
        layouts::{
            CHANNEL_LAYOUT_MONO, CHANNEL_LAYOUT_OGG_3P0, CHANNEL_LAYOUT_OGG_4P0,
            CHANNEL_LAYOUT_OGG_5P0, CHANNEL_LAYOUT_OGG_5P1, CHANNEL_LAYOUT_OGG_6P1,
            CHANNEL_LAYOUT_OGG_7P1, CHANNEL_LAYOUT_STEREO,
        },
    },
    codecs::{
        CodecParameters,
        audio::{AudioCodecParameters, well_known::CODEC_ID_OPUS},
    },
    errors::{Error, Result},
    formats::{
        FormatInfo, FormatOptions, FormatReader, MediaInfo, SeekMode, SeekTo, SeekedTo, Track,
        probe::{ProbeDataMatchSpec, ProbeFormatData, ProbeableFormat, Score, Scoreable},
        well_known::FORMAT_ID_OGG,
    },
    io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions, ReadBytes, ScopedStream},
    meta::{Metadata, MetadataLog},
    packet::Packet,
    units::{TimeBase, Timestamp},
};

// Mapping family one defines Vorbis/Opus speaker semantics. Symphonia's Ogg
// layouts describe the speaker set; the decoder must reorder into canonical planes.
// Family 255 has no speaker semantics, so never invent a surround layout for it.
pub(crate) const fn opus_channels(head: &moosicbox_ogg_opus::OpusHead) -> Result<Channels> {
    match (head.mapping_family, head.channels) {
        (0 | 1, 1) => Ok(CHANNEL_LAYOUT_MONO),
        (0 | 1, 2) => Ok(CHANNEL_LAYOUT_STEREO),
        (1, 3) => Ok(CHANNEL_LAYOUT_OGG_3P0),
        (1, 4) => Ok(CHANNEL_LAYOUT_OGG_4P0),
        (1, 5) => Ok(CHANNEL_LAYOUT_OGG_5P0),
        (1, 6) => Ok(CHANNEL_LAYOUT_OGG_5P1),
        (1, 7) => Ok(CHANNEL_LAYOUT_OGG_6P1),
        (1, 8) => Ok(CHANNEL_LAYOUT_OGG_7P1),
        _ => Err(Error::Unsupported("unsupported Opus speaker mapping")),
    }
}

const fn time_base() -> TimeBase {
    TimeBase::from_recip(NonZeroU32::new(48_000).unwrap())
}

const INFO: FormatInfo = FormatInfo {
    format: FORMAT_ID_OGG,
    short_name: "ogg-opus",
    long_name: "Ogg Opus",
};

/// Ogg Opus format reader backed by the Symphonia-independent container library.
///
/// The initial section is configured during probing, including on nonseekable
/// sources. Each subsequent logical section updates track 0 and makes
/// `next_packet` return [`Error::ResetRequired`] once, before its first packet.
/// Callers must reselect the track and recreate the decoder even when headers
/// match. Retain the global seek cutoff: packet timestamps are cumulative audible
/// chain positions, not section-local positions. Seeking across sections follows
/// the same reset contract; same-section seeks require the caller's normal reset.
pub struct OggOpusFormatReader<'s> {
    reader: OggOpusChainReader<MediaSourceStream<'s>>,
    section: ChainSection,
    sections: Vec<(ChainSection, u64)>,
    total: Option<u64>,
    skip_start: bool,
    tracks: Vec<Track>,
    info: MediaInfo,
    metadata: MetadataLog,
}

// Preserve both the chain reader's unread buffer and the underlying Symphonia
// stream. Extraction is infallible: no implicit rewind can fail or lose bytes.
// A terminal chain failure may have consumed its source; retain that failure as
// an I/O error instead of fabricating an empty, successful source.
struct ChainSource<'s>(Option<BufReader<MediaSourceStream<'s>>>);

impl Read for ChainSource<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.0
            .as_mut()
            .ok_or_else(|| io::Error::other("Opus chain source unavailable"))?
            .read(output)
    }
}

impl Seek for ChainSource<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.0
            .as_mut()
            .ok_or_else(|| io::Error::other("Opus chain source unavailable"))?
            .seek(position)
    }
}

impl MediaSource for ChainSource<'_> {
    fn is_seekable(&self) -> bool {
        self.0
            .as_ref()
            .is_some_and(|source| source.get_ref().is_seekable())
    }
    fn byte_len(&self) -> Option<u64> {
        self.0
            .as_ref()
            .and_then(|source| source.get_ref().byte_len())
    }
}

fn section_track(section: &ChainSection, total: Option<u64>) -> Result<Track> {
    let head = &section.head;
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_OPUS)
        .with_sample_rate(48_000)
        .with_channels(opus_channels(head)?)
        .with_extra_data(head.encode()?.into_boxed_slice());
    let mut track = Track::new(0);
    track
        .with_delay(u32::from(head.pre_skip))
        .with_codec_params(CodecParameters::Audio(params))
        .with_time_base(time_base());
    if let Some(total) = total {
        track.with_duration(total.into());
    }
    Ok(track)
}

impl Scoreable for OggOpusFormatReader<'_> {
    fn score(mut src: ScopedStream<&mut MediaSourceStream<'_>>) -> Result<Score> {
        // Identify the first packet, not merely OggS: Vorbis/FLAC must retain their builtin reader.
        let mut header = [0; 27];
        src.read_buf_exact(&mut header)?;
        if &header[..4] != b"OggS" || header[26] == 0 {
            return Ok(Score::Unsupported);
        }
        let mut lacing = vec![0; usize::from(header[26])];
        src.read_buf_exact(&mut lacing)?;
        if lacing[0] < 19 {
            return Ok(Score::Unsupported);
        }
        let mut signature = [0; 8];
        src.read_buf_exact(&mut signature)?;
        Ok(if &signature == b"OpusHead" {
            Score::Supported(255)
        } else {
            Score::Unsupported
        })
    }
}

impl ProbeableFormat<'_> for OggOpusFormatReader<'_> {
    fn try_probe_new(
        mss: MediaSourceStream<'_>,
        opts: FormatOptions,
    ) -> Result<Box<dyn FormatReader + '_>> {
        let seekable = mss.is_seekable();
        let mut reader = OggOpusChainReader::new(mss)?;
        let Some(ChainEvent::SectionStart(section)) = reader.next_event()? else {
            return Err(Error::DecodeError("missing Opus chain section"));
        };
        let mut sections = Vec::new();
        let total = if seekable {
            let mut current = Some(section.clone());
            let mut total = 0_u64;
            while let Some(event) = reader.next_event()? {
                match event {
                    ChainEvent::SectionStart(next) => current = Some(next),
                    ChainEvent::SectionEnd { samples, .. } => {
                        total = total
                            .checked_add(samples)
                            .ok_or(Error::DecodeError("Opus chain duration overflow"))?;
                        let completed = current
                            .take()
                            .ok_or(Error::DecodeError("missing Opus chain section"))?;
                        sections.push((completed, samples));
                    }
                    ChainEvent::Packet(_) => {}
                }
            }
            reader.seek_to_section(0, 0)?;
            Some(total)
        } else {
            None
        };
        let track = section_track(&section, total)?;
        let info = MediaInfo::from_track(&track);
        Ok(Box::new(OggOpusFormatReader {
            reader,
            section,
            sections,
            total,
            skip_start: seekable,
            tracks: vec![track],
            info,
            metadata: opts.external_data.metadata.unwrap_or_default(),
        }))
    }
    fn probe_data() -> &'static [ProbeFormatData] {
        &[ProbeFormatData {
            info: INFO,
            spec: ProbeDataMatchSpec {
                extensions: &["opus", "ogg", "oga"],
                mime_types: &["audio/opus", "audio/ogg"],
                markers: &[b"OggS"],
            },
        }]
    }
}

impl FormatReader for OggOpusFormatReader<'_> {
    fn format_info(&self) -> &FormatInfo {
        &INFO
    }
    fn media_info(&self) -> &MediaInfo {
        &self.info
    }
    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }
    fn tracks(&self) -> &[Track] {
        &self.tracks
    }
    fn next_packet(&mut self) -> Result<Option<Packet>> {
        let audio = loop {
            match self.reader.next_event()? {
                Some(ChainEvent::SectionStart(section)) => {
                    let track = section_track(&section, self.total)?;
                    self.info = MediaInfo::from_track(&track);
                    self.tracks = vec![track];
                    self.section = section;
                    if self.skip_start {
                        self.skip_start = false;
                    } else {
                        // Every section requires a fresh decoder, even when headers match.
                        return Err(Error::ResetRequired);
                    }
                }
                Some(ChainEvent::Packet(audio)) => break audio,
                Some(ChainEvent::SectionEnd { .. }) => {}
                None => return Ok(None),
            }
        };
        let relative = audio
            .decoded_start
            .checked_sub(self.reader.granule_origin().unwrap_or(0))
            .ok_or(Error::DecodeError("Opus timestamp before origin"))?;
        let start = i64::try_from(relative)
            .map_err(|_| Error::DecodeError("Opus timestamp overflow"))?
            .checked_sub(i64::from(self.section.head.pre_skip))
            .and_then(|local| {
                i64::try_from(self.section.audible_start)
                    .ok()
                    .and_then(|offset| local.checked_add(offset))
            })
            .ok_or(Error::DecodeError("Opus chain timestamp overflow"))?;
        let mut packet = Packet::new(
            0,
            Timestamp::new(start),
            u64::from(audio.audible_samples()).into(),
            audio.data,
        );
        packet.trim_start = u64::from(audio.trim_start).into();
        packet.trim_end = u64::from(audio.trim_end).into();
        Ok(Some(packet))
    }
    fn seek(&mut self, _mode: SeekMode, to: SeekTo) -> Result<SeekedTo> {
        let ts = match to {
            SeekTo::Timestamp { ts, track_id: 0 } => ts,
            SeekTo::Time {
                time,
                track_id: None | Some(0),
            } => time_base()
                .calc_timestamp(time)
                .ok_or(Error::DecodeError("Opus seek overflow"))?,
            _ => return Err(Error::Unsupported("unknown Opus track")),
        };
        let sample = u64::try_from(ts.get()).unwrap_or(0);
        let (section, duration) = self
            .sections
            .iter()
            .rev()
            .find(|(section, _)| section.audible_start <= sample)
            .ok_or(Error::Unsupported(
                "Opus chain seeking requires seekable source",
            ))?;
        let index = section.index;
        let local = sample.saturating_sub(section.audible_start).min(*duration);
        let result = self.reader.seek_to_section(index, local)?;
        self.skip_start = index == self.section.index;
        // The next SectionStart publishes configuration and requests decoder
        // replacement when seeking across sections. Same-section callers reset.
        let actual_sample = result
            .section
            .audible_start
            .checked_add(result.local.sample)
            .ok_or(Error::DecodeError("Opus seek overflow"))?;
        let actual = Timestamp::new(
            i64::try_from(actual_sample).map_err(|_| Error::DecodeError("Opus seek overflow"))?,
        );
        Ok(SeekedTo {
            track_id: 0,
            required_ts: ts,
            actual_ts: actual,
        })
    }
    fn into_inner<'s>(self: Box<Self>) -> MediaSourceStream<'s>
    where
        Self: 's,
    {
        MediaSourceStream::new(
            Box::new(ChainSource(self.reader.into_inner())),
            MediaSourceStreamOptions::default(),
        )
    }
}

#[cfg(test)]
mod extraction_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn extraction_preserves_prefetched_bytes_and_seek_position() {
        let bytes: Vec<u8> = (0..200).collect();
        let source = MediaSourceStream::new(
            Box::new(Cursor::new(bytes.clone())),
            MediaSourceStreamOptions::default(),
        );
        let mut buffered = BufReader::with_capacity(64, source);
        let mut prefix = [0; 7];
        buffered.read_exact(&mut prefix).unwrap();
        assert_ne!(buffered.buffer(), []);
        let mut extracted = MediaSourceStream::new(
            Box::new(ChainSource(Some(buffered))),
            MediaSourceStreamOptions::default(),
        );
        let mut remainder = Vec::new();
        extracted.read_to_end(&mut remainder).unwrap();
        assert_eq!(remainder, bytes[7..]);
        extracted.seek(SeekFrom::Start(3)).unwrap();
        extracted.read_exact(&mut prefix).unwrap();
        assert_eq!(prefix, bytes[3..10]);
    }

    #[test]
    fn unavailable_source_reports_error_not_clean_eof() {
        let mut source = ChainSource(None);
        assert!(source.read(&mut [0; 1]).is_err());
        assert!(source.seek(SeekFrom::Start(0)).is_err());
        assert!(!source.is_seekable());
        assert_eq!(source.byte_len(), None);
    }
}
