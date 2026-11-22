#![cfg(feature = "opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Streaming lifecycle regressions independent of a C encoder or reference process.
use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusStreamEncoder,
};
use std::io::{self, Cursor, Write};

fn config() -> OpusEncodeConfig {
    OpusEncodeConfig {
        backend: OpusEncodeBackend::Native,
        ..Default::default()
    }
}

#[derive(Default)]
struct ShortSink {
    bytes: Vec<u8>,
    interrupted: bool,
}
impl Write for ShortSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.interrupted = !self.interrupted;
        if self.interrupted {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let count = bytes.len().min(7);
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn arbitrary_chunks_partial_writes_and_exact_2457_duration() {
    let pcm: Vec<_> = (0..2457 * 2)
        .map(|i| (i as f32 * 0.03).sin() * 0.1)
        .collect();
    let mut whole = OpusStreamEncoder::new(Vec::new(), config()).unwrap();
    whole.write(&pcm).unwrap();
    let expected = whole.finish().unwrap().unwrap();
    let mut chunked = OpusStreamEncoder::new(ShortSink::default(), config()).unwrap();
    chunked.write(&[]).unwrap();
    for chunk in pcm.chunks(34) {
        chunked.write(chunk).unwrap();
    }
    let bytes = chunked.finish().unwrap().unwrap().bytes;
    assert_eq!(bytes, expected);
    assert!(chunked.finish().unwrap().is_none());
    assert!(chunked.write(&[]).is_err());
    let mut reader = moosicbox_ogg_opus::OggOpusReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.total_samples().unwrap(), 2457);
}

#[test]
fn empty_stream_finishes_once_with_zero_duration() {
    let mut encoder = OpusStreamEncoder::new(Vec::new(), config()).unwrap();
    encoder.write(&[]).unwrap();
    let bytes = encoder.finish().unwrap().unwrap();
    assert!(encoder.finish().unwrap().is_none());
    let mut reader = moosicbox_ogg_opus::OggOpusReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.total_samples().unwrap(), 0);
}

#[test]
fn invalid_input_does_not_consume_pcm_or_poison_stream() {
    let mut encoder = OpusStreamEncoder::new(Vec::new(), config()).unwrap();
    assert!(encoder.write(&[0.0]).is_err());
    assert!(encoder.write(&[f32::NAN, 0.0]).is_err());
    assert!(encoder.write(&[0.0, f32::INFINITY]).is_err());
    encoder.write(&[0.0; 14]).unwrap();
    let bytes = encoder.finish().unwrap().unwrap();
    let mut reader = moosicbox_ogg_opus::OggOpusReader::new(Cursor::new(bytes)).unwrap();
    assert_eq!(reader.total_samples().unwrap(), 7);
}

struct FailingSink {
    remaining: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    kind: io::ErrorKind,
}
impl Write for FailingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.remaining.load(std::sync::atomic::Ordering::Relaxed);
        if remaining == 0 {
            return if self.kind == io::ErrorKind::WriteZero {
                Ok(0)
            } else {
                Err(self.kind.into())
            };
        }
        let count = remaining.min(bytes.len());
        self.remaining
            .fetch_sub(count, std::sync::atomic::Ordering::Relaxed);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn auto_is_distinct_from_default_and_controls_require_complete_frames() {
    use moosicbox_audio_encoder::opus_stream::{EncoderMode, OpusEncodeMode};
    let mut encoder = OpusStreamEncoder::new(
        Vec::new(),
        OpusEncodeConfig {
            mode: OpusEncodeMode::Auto,
            ..config()
        },
    )
    .unwrap();
    assert_eq!(encoder.config().mode, OpusEncodeMode::Auto);
    assert_eq!(encoder.control().unwrap().mode, EncoderMode::Auto);
    let control = encoder.control().unwrap();
    encoder.set_control(control).unwrap();
    encoder.write(&[0.0; 2]).unwrap();
    assert!(encoder.set_control(control).is_err());
    encoder
        .write(&vec![0.0; (config().frame_size - 1) * 2])
        .unwrap();
    encoder.set_control(control).unwrap();
    encoder.finish().unwrap();
    assert!(encoder.set_control(control).is_err());
    let default = OpusStreamEncoder::new(Vec::new(), config()).unwrap();
    assert_eq!(default.config().mode, OpusEncodeMode::Default);
    assert_eq!(default.control().unwrap().mode, EncoderMode::Celt);
}

#[test]
fn multistream_layout_validation_precedes_headers_and_chunks_preserve_duration() {
    use moosicbox_audio_encoder::opus_stream::OpusMultistreamEncodeConfig;
    use moosicbox_opus_native::multistream::MultistreamLayout;
    for (family, streams, coupled, mapping) in
        [(1, 2, 1, vec![0, 2, 1]), (255, 2, 0, vec![1, 0, 255])]
    {
        let config = OpusMultistreamEncodeConfig {
            sample_rate: 48000,
            frame_size: 960,
            layout: MultistreamLayout::new(streams, coupled, mapping).unwrap(),
            bitrates: vec![64000; streams as usize],
            mapping_family: family,
        };
        let mut encoder = OpusStreamEncoder::new_multistream(Vec::new(), config.clone()).unwrap();
        for chunk in [1, 27, 960, 1469] {
            encoder.write(&vec![0.0; chunk * 3]).unwrap();
        }
        let bytes = encoder.finish().unwrap().unwrap();
        assert!(encoder.finish().unwrap().is_none());
        let mut reader = moosicbox_ogg_opus::OggOpusReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.total_samples().unwrap(), 2457);
        let mut invalid = config;
        invalid.mapping_family = if family == 1 { 0 } else { 1 };
        let mut sink = Vec::new();
        assert!(OpusStreamEncoder::new_multistream(&mut sink, invalid).is_err());
        assert!(sink.is_empty());
    }
}

#[test]
fn final_sink_flush_failure_is_not_success_on_retry() {
    struct FlushFailure;
    impl Write for FlushFailure {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    let mut encoder = OpusStreamEncoder::new(FlushFailure, config()).unwrap();
    encoder.write(&[0.0; 14]).unwrap();
    assert_eq!(
        encoder.finish().err().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        encoder.finish().err().unwrap().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        encoder.write(&[]).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[test]
fn output_failure_is_terminal_during_write_and_finish() {
    for kind in [
        io::ErrorKind::BrokenPipe,
        io::ErrorKind::WouldBlock,
        io::ErrorKind::WriteZero,
    ] {
        for finish in [false, true] {
            let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(usize::MAX));
            let mut encoder = OpusStreamEncoder::new(
                FailingSink {
                    remaining: remaining.clone(),
                    kind,
                },
                config(),
            )
            .unwrap();
            // Fail after a partial page, regardless of header/tag or packet length.
            remaining.store(3, std::sync::atomic::Ordering::Relaxed);
            let error = if finish {
                encoder.write(&[0.0; 14]).unwrap();
                encoder.finish().err().expect("tail must fail")
            } else {
                encoder.write(&vec![0.0; 960 * 2 * 5]).unwrap_err()
            };
            assert_eq!(error.kind(), kind);
            assert_eq!(encoder.write(&[]).unwrap_err().kind(), kind);
            assert_eq!(encoder.finish().err().unwrap().kind(), kind);
            assert_eq!(encoder.finish().err().unwrap().kind(), kind);
        }
    }
}
