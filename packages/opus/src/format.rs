//! Symphonia adapter for the standalone Ogg Opus container reader.

use moosicbox_ogg_opus::OggOpusReader;
use std::num::NonZeroU32;
use symphonia::core::{
    audio::layouts::{CHANNEL_LAYOUT_MONO, CHANNEL_LAYOUT_STEREO},
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
    io::{MediaSource, MediaSourceStream, ReadBytes, ScopedStream},
    meta::{Metadata, MetadataLog},
    packet::Packet,
    units::{TimeBase, Timestamp},
};

const fn time_base() -> TimeBase {
    TimeBase::from_recip(NonZeroU32::new(48_000).unwrap())
}

const INFO: FormatInfo = FormatInfo {
    format: FORMAT_ID_OGG,
    short_name: "ogg-opus",
    long_name: "Ogg Opus",
};

/// Ogg Opus format reader backed by the Symphonia-independent container library.
pub struct OggOpusFormatReader<'s> {
    reader: OggOpusReader<MediaSourceStream<'s>>,
    tracks: Vec<Track>,
    info: MediaInfo,
    metadata: MetadataLog,
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
        let mut reader = OggOpusReader::new(mss)?;
        let total = if seekable {
            Some(reader.total_samples()?)
        } else {
            None
        };
        let head = reader.head();
        let channels = match head.channels {
            1 => CHANNEL_LAYOUT_MONO,
            2 => CHANNEL_LAYOUT_STEREO,
            _ => return Err(Error::Unsupported("multichannel Opus mapping")),
        };
        // Preserve the Opus identification metadata for codec validation and output gain.
        let extra = head.encode()?;
        let mut params = AudioCodecParameters::new();
        params
            .for_codec(CODEC_ID_OPUS)
            .with_sample_rate(48_000)
            .with_channels(channels)
            .with_extra_data(extra.into_boxed_slice());
        let mut track = Track::new(0);
        track.with_delay(u32::from(head.pre_skip));
        track
            .with_codec_params(CodecParameters::Audio(params))
            .with_time_base(time_base());
        if let Some(total) = total {
            track.with_duration(total.into());
        }
        let info = MediaInfo::from_track(&track);
        Ok(Box::new(OggOpusFormatReader {
            reader,
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
        let Some(audio) = self.reader.next_packet()? else {
            return Ok(None);
        };
        let relative = audio
            .decoded_start
            .checked_sub(self.reader.granule_origin().unwrap_or(0))
            .ok_or(Error::DecodeError("Opus timestamp before origin"))?;
        let start = i64::try_from(relative)
            .map_err(|_| Error::DecodeError("Opus timestamp overflow"))?
            - i64::from(self.reader.head().pre_skip);
        let mut packet = Packet::new(
            0,
            Timestamp::new(start),
            u64::from(audio.duration).into(),
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
        let result = self.reader.seek_to(u64::try_from(ts.get()).unwrap_or(0))?;
        // Return the target while retaining preroll packets. Product playback decodes and
        // discards those packets before delivering samples at the requested timestamp.
        let actual = Timestamp::new(
            i64::try_from(result.sample).map_err(|_| Error::DecodeError("Opus seek overflow"))?,
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
        self.reader.into_inner()
    }
}
