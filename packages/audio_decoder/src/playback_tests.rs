//! Exercise real MP3 decoding with demuxer delay/seek metadata, through both APIs.
use super::*;
use std::sync::{Arc, Mutex};
use symphonia::core::{
    audio::Audio,
    codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_MP3},
    formats::{FORMAT_ID_NULL, FormatInfo, MediaInfo, SeekedTo},
    meta::{Metadata, MetadataLog},
    units::TimeBase,
};

struct Reader {
    track: Track,
    packet: Option<Packet>,
    metadata: MetadataLog,
    info: MediaInfo,
    target: i64,
    terminal_error: bool,
}
impl Reader {
    fn new(target: i64) -> Self {
        let params = AudioCodecParameters::new().for_codec(CODEC_ID_MP3).clone();
        let mut track = Track::new(0);
        track.with_codec_params(CodecParameters::Audio(params));
        track.with_time_base(TimeBase::try_new(1, 44100).unwrap());
        track.with_delay(576);
        let mut packet = Packet::new(
            0,
            Timestamp::new(-576),
            Duration::new(576),
            include_bytes!("../tests/fixtures/delay_frame.mp3").to_vec(),
        );
        packet.trim_start = Duration::new(576);
        Self {
            track,
            packet: Some(packet),
            metadata: MetadataLog::default(),
            info: MediaInfo::default(),
            target,
            terminal_error: false,
        }
    }
}
impl FormatReader for Reader {
    fn format_info(&self) -> &FormatInfo {
        &FormatInfo {
            format: FORMAT_ID_NULL,
            short_name: "mp3-test",
            long_name: "MP3 delay packet",
        }
    }
    fn media_info(&self) -> &MediaInfo {
        &self.info
    }
    fn metadata(&mut self) -> Metadata<'_> {
        self.metadata.metadata()
    }
    fn seek(&mut self, _: SeekMode, _: SeekTo) -> symphonia::core::errors::Result<SeekedTo> {
        Ok(SeekedTo {
            track_id: 0,
            required_ts: Timestamp::new(self.target),
            actual_ts: Timestamp::new(-576),
        })
    }
    fn tracks(&self) -> &[Track] {
        std::slice::from_ref(&self.track)
    }
    fn next_packet(&mut self) -> symphonia::core::errors::Result<Option<Packet>> {
        if self.packet.is_none() && self.terminal_error {
            return Err(Error::Unsupported("injected worker failure after PCM"));
        }
        Ok(self.packet.take())
    }
    fn into_inner<'s>(self: Box<Self>) -> MediaSourceStream<'s>
    where
        Self: 's,
    {
        MediaSourceStream::new(
            Box::new(std::io::Cursor::new(Vec::<u8>::new())),
            MediaSourceStreamOptions::default(),
        )
    }
}

struct Output(Arc<Mutex<Vec<f32>>>);
impl AudioDecode for Output {
    fn decoded(
        &mut self,
        buf: AudioBuffer<f32>,
        _: &Packet,
        _: &Track,
    ) -> Result<(), AudioDecodeError> {
        self.0
            .lock()
            .unwrap()
            .extend_from_slice(buf.plane(0).unwrap());
        Ok(())
    }
}
fn playback(sync: bool, gapless: bool, target: Option<i64>) -> Vec<f32> {
    let reader = Box::new(Reader::new(target.unwrap_or(0)));
    let mut opts = AudioDecoderOptions::default();
    opts.gapless = gapless;
    let seek = target.map(|_| 0.0);
    if sync {
        let output = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&output);
        let mut handler = AudioDecodeHandler::new().with_output(Box::new(move |_, _| {
            Ok(Box::new(Output(Arc::clone(&sink))))
        }));
        assert_eq!(decode(reader, &mut handler, None, seek, opts).unwrap(), 0);
        output.lock().unwrap().clone()
    } else {
        unsync::decode(reader, None, seek, opts)
            .unwrap()
            .iter()
            .flat_map(|buf| buf.plane(0).unwrap().to_vec())
            .collect()
    }
}
#[test_log::test]
fn mp3_negative_pts_preserves_valid_frames_and_sample_accurate_seeking() {
    for sync in [true, false] {
        let raw = playback(sync, false, None);
        let valid = playback(sync, true, None);
        assert_eq!(raw.len(), 1152);
        assert_eq!(valid.len(), 576);
        assert_eq!(valid, raw[576..]);
        assert!(valid.iter().any(|sample| *sample != 0.0));
        assert_eq!(playback(sync, true, Some(0)), valid);
        assert_eq!(playback(sync, false, Some(0)), valid);
        assert_eq!(playback(sync, true, Some(100)), valid[100..]);
        assert_eq!(playback(sync, false, Some(100)), valid[100..]);
        assert_eq!(playback(sync, true, Some(576)), Vec::<f32>::new());
        assert_eq!(playback(sync, false, Some(576)), Vec::<f32>::new());
    }
}

#[test]
fn seek_uses_track_time_base_and_rounds_to_next_sample() {
    let mut reader = Reader::new(0);
    reader.track.time_base = TimeBase::try_new(1, 1000);
    let packet = reader.packet.as_mut().unwrap();
    packet.pts = Timestamp::new(0);
    packet.trim_start = Duration::ZERO;
    assert_eq!(
        seek_frames(
            packet,
            &reader.track,
            Some(Timestamp::new(1)),
            44100,
            576,
            true
        ),
        45
    );
}

#[test]
fn result_receiver_delivers_terminal_worker_failure_after_pcm() {
    let mut reader = Reader::new(0);
    reader.terminal_error = true;
    let items: Vec<_> =
        unsync::decode_with_options_result(Box::new(reader), None, None, DecodeOptions::default())
            .unwrap()
            .iter()
            .collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_ref().unwrap().frames(), 576);
    assert!(matches!(
        &items[1],
        Err(DecodeError::Symphonia(Error::Unsupported(
            "injected worker failure after PCM"
        )))
    ));
    let items: Vec<_> = unsync::decode_with_options_result(
        Box::new(Reader::new(0)),
        None,
        None,
        DecodeOptions::default(),
    )
    .unwrap()
    .iter()
    .collect();
    assert_eq!(items.len(), 1);
    assert!(items[0].is_ok());
}
