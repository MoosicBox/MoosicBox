#![cfg(feature = "opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Selected output lifecycle and external writer failures.
use moosicbox_audio_encoder::opus_stream::{OpusEncodeBackend, OpusEncodeConfig};
use moosicbox_audio_output::{
    AudioWrite,
    encoder::{AudioEncoder, opus::SelectedOpusEncoder},
};
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};
use symphonia::core::audio::{AudioBuffer, AudioMut, AudioSpec, layouts::CHANNEL_LAYOUT_STEREO};

fn config() -> OpusEncodeConfig {
    OpusEncodeConfig {
        backend: OpusEncodeBackend::Native,
        ..Default::default()
    }
}
fn audio(rate: u32, frames: usize) -> AudioBuffer<f32> {
    let mut buffer = AudioBuffer::new(AudioSpec::new(rate, CHANNEL_LAYOUT_STEREO), frames.max(1));
    buffer.render_silence(Some(frames));
    buffer
}
#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
}
struct Sink {
    state: Arc<Mutex<State>>,
    fail_write: bool,
    fail_flush: bool,
}
impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.state.lock().unwrap();
        state.writes += 1;
        if self.fail_write && state.writes > 1 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let count = bytes.len().min(5);
        state.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.state.lock().unwrap().flushes += 1;
        if self.fail_flush {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        }
    }
}
#[test]
fn public_auto_controls_keep_output_clock_and_resampler_guard_transactional() {
    use moosicbox_audio_encoder::opus_stream::{EncoderMode, OpusEncodeMode};

    let auto = OpusEncodeConfig {
        mode: OpusEncodeMode::Auto,
        bitrate: 192_000,
        ..config()
    };
    let mut encoder = SelectedOpusEncoder::new(auto).unwrap();
    let mut control = encoder.control().unwrap();
    let mut bytes = encoder.encode(audio(48_000, 960)).unwrap().to_vec();
    assert_eq!(encoder.active_mode(), Some(EncoderMode::Hybrid));
    for (bitrate, frames) in [(64_000, 960), (32_000, 537)] {
        control.bitrate = bitrate;
        encoder.set_control(control).unwrap();
        bytes.extend_from_slice(&encoder.encode(audio(48_000, frames)).unwrap());
        assert_eq!(encoder.active_mode(), Some(EncoderMode::Hybrid));
    }
    let before = encoder.control();
    assert!(encoder.set_control(control).is_err());
    assert_eq!(encoder.control(), before);
    bytes.extend_from_slice(&encoder.finish().unwrap());
    assert!(encoder.finish().unwrap().is_empty());
    assert!(encoder.set_control(control).is_err());
    let eos = bytes
        .windows(4)
        .rposition(|window| window == b"OggS")
        .unwrap();
    let granule = u64::from_le_bytes(bytes[eos + 6..eos + 14].try_into().unwrap());
    let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
    assert_eq!(granule - u64::from(pre_skip), 2457);

    let encode_resampled = |reject_control| {
        let mut encoder = SelectedOpusEncoder::new(auto).unwrap();
        let mut bytes = encoder.encode(audio(44_100, 1)).unwrap().to_vec();
        if reject_control {
            let before = encoder.control();
            assert!(encoder.set_control(control).is_err());
            assert_eq!(encoder.control(), before);
        }
        bytes.extend_from_slice(&encoder.encode(audio(44_100, 2456)).unwrap());
        bytes.extend_from_slice(&encoder.finish().unwrap());
        bytes
    };
    let bytes = encode_resampled(true);
    assert_eq!(bytes, encode_resampled(false));
    let eos = bytes
        .windows(4)
        .rposition(|window| window == b"OggS")
        .unwrap();
    let granule = u64::from_le_bytes(bytes[eos + 6..eos + 14].try_into().unwrap());
    let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
    assert_eq!(granule - u64::from(pre_skip), 2457 * 48_000 / 44_100);
}

#[test]
fn empty_input_does_not_pin_resampler_or_input_spec() {
    let mut encoder = SelectedOpusEncoder::new(config()).unwrap();
    let mut bytes = encoder.encode(audio(44100, 0)).unwrap().to_vec();
    bytes.extend_from_slice(&encoder.encode(audio(48000, 2457)).unwrap());
    bytes.extend_from_slice(&encoder.finish().unwrap());
    assert!(encoder.finish().unwrap().is_empty());
    // EOS page carries pre-skip plus the exact audible duration.
    let last = bytes
        .windows(4)
        .rposition(|window| window == b"OggS")
        .unwrap();
    let granule = u64::from_le_bytes(bytes[last + 6..last + 14].try_into().unwrap());
    let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
    assert_eq!(granule - u64::from(pre_skip), 2457);
}
#[test]
fn resampled_arbitrary_chunks_preserve_exact_audible_clock() {
    for chunks in [vec![2457], vec![1, 7, 19, 2430], vec![960, 960, 537]] {
        let mut encoder = SelectedOpusEncoder::new(config()).unwrap();
        let mut bytes = Vec::new();
        for frames in chunks {
            bytes.extend_from_slice(&encoder.encode(audio(44100, frames)).unwrap());
        }
        bytes.extend_from_slice(&encoder.finish().unwrap());
        let last = bytes
            .windows(4)
            .rposition(|window| window == b"OggS")
            .unwrap();
        let granule = u64::from_le_bytes(bytes[last + 6..last + 14].try_into().unwrap());
        let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
        assert_eq!(granule - u64::from(pre_skip), 2457 * 48000 / 44100);
    }
}

#[test]
fn short_writes_match_direct_output() {
    let mut direct = SelectedOpusEncoder::new(config()).unwrap();
    let mut expected = direct.encode(audio(48000, 2457)).unwrap().to_vec();
    expected.extend_from_slice(&direct.finish().unwrap());
    let state = Arc::new(Mutex::new(State::default()));
    let mut writer = SelectedOpusEncoder::with_writer(
        Sink {
            state: state.clone(),
            fail_write: false,
            fail_flush: false,
        },
        config(),
    )
    .unwrap();
    AudioWrite::write(&mut writer, audio(48000, 2457)).unwrap();
    AudioWrite::flush(&mut writer).unwrap();
    let count = state.lock().unwrap().bytes.len();
    AudioWrite::flush(&mut writer).unwrap();
    assert_eq!(state.lock().unwrap().bytes.len(), count);
    assert_eq!(state.lock().unwrap().bytes, expected);
}
#[test]
fn family1_positioned_pcm_is_reordered_before_multistream_encoding() {
    use moosicbox_audio_encoder::opus_stream::OpusStreamEncoder;
    use moosicbox_audio_output::encoder::opus::{MultistreamLayout, OpusMultistreamEncodeConfig};
    use symphonia::core::audio::{Audio, layouts::CHANNEL_LAYOUT_3P0};
    let config = OpusMultistreamEncodeConfig {
        sample_rate: 48000,
        frame_size: 960,
        layout: MultistreamLayout::new(2, 1, vec![0, 2, 1]).unwrap(),
        bitrates: vec![64000, 64000],
        mapping_family: 1,
    };
    let mut selected = SelectedOpusEncoder::new_multistream(config.clone()).unwrap();
    let mut audio = AudioBuffer::<f32>::new(AudioSpec::new(48000, CHANNEL_LAYOUT_3P0), 2457);
    audio.render_silence(Some(2457));
    for channel in 0..3 {
        for (frame, sample) in audio.plane_mut(channel).unwrap().iter_mut().enumerate() {
            *sample = (frame as f32 * (channel + 1) as f32 * 0.03).sin() * 0.1;
        }
    }
    let mut pcm = Vec::new();
    for frame in 0..2457 {
        for channel in [0, 2, 1] {
            pcm.push(audio.plane(channel).unwrap()[frame]);
        }
    }
    let mut direct = OpusStreamEncoder::new_multistream(Vec::new(), config).unwrap();
    direct.write(&pcm).unwrap();
    let expected = direct.finish().unwrap().unwrap();
    let mut actual = selected.encode(audio).unwrap().to_vec();
    actual.extend_from_slice(&selected.finish().unwrap());
    assert_eq!(actual, expected);
}

#[test]
fn partial_write_and_final_flush_errors_are_terminal() {
    for fail_write in [true, false] {
        let state = Arc::new(Mutex::new(State::default()));
        let mut encoder = SelectedOpusEncoder::with_writer(
            Sink {
                state: state.clone(),
                fail_write,
                fail_flush: !fail_write,
            },
            config(),
        )
        .unwrap();
        if fail_write {
            assert!(AudioWrite::write(&mut encoder, audio(48000, 2457)).is_err());
        } else {
            AudioWrite::write(&mut encoder, audio(48000, 2457)).unwrap();
            assert!(AudioWrite::flush(&mut encoder).is_err());
        }
        let writes = state.lock().unwrap().writes;
        assert!(encoder.encode(audio(48000, 1)).is_err());
        assert!(encoder.finish().is_err());
        assert!(AudioWrite::flush(&mut encoder).is_err());
        assert_eq!(state.lock().unwrap().writes, writes);
    }
}
