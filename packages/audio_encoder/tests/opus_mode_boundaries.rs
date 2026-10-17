#![cfg(feature = "opus-native")]
//! Explicit mode support boundaries must reject before touching the Ogg sink.
use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode, OpusStreamEncoder,
};
use std::io::{self, Write};

#[derive(Default)]
struct Sink(usize);
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn native_mode_bitrate_boundaries_validate_before_headers() {
    for (mode, channels, frame_size, minimum, maximum) in [
        (OpusEncodeMode::Celt, 1, 120, 128000, 256000),
        (OpusEncodeMode::Celt, 2, 240, 64000, 256000),
        (OpusEncodeMode::Celt, 1, 480, 32000, 256000),
        (OpusEncodeMode::Celt, 2, 960, 32000, 256000),
        (OpusEncodeMode::Silk, 1, 160, 48000, 128000),
        (OpusEncodeMode::Silk, 2, 320, 96000, 128000),
        (OpusEncodeMode::Hybrid, 1, 480, 128000, 256000),
        (OpusEncodeMode::Hybrid, 2, 960, 128000, 256000),
    ] {
        let base = OpusEncodeConfig {
            backend: OpusEncodeBackend::Native,
            mode,
            channels,
            frame_size,
            sample_rate: if mode == OpusEncodeMode::Silk {
                16000
            } else {
                48000
            },
            ..Default::default()
        };
        for bitrate in [minimum - 1, maximum + 1] {
            let mut sink = Sink::default();
            assert!(
                OpusStreamEncoder::new(&mut sink, OpusEncodeConfig { bitrate, ..base }).is_err(),
                "{base:?} bitrate={bitrate}"
            );
            assert_eq!(sink.0, 0, "invalid configuration wrote headers");
        }
        for bitrate in [minimum, maximum] {
            assert!(
                OpusStreamEncoder::new(Sink::default(), OpusEncodeConfig { bitrate, ..base })
                    .is_ok(),
                "{base:?} bitrate={bitrate}"
            );
        }
    }
}
