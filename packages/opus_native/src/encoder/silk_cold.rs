//! Exact captured cold-entry prefix qualification (test-only).
use super::*;

#[test]
#[ignore = "requires captured adaptive_low f32 files"]
fn captured_cold_prefix_quality() {
    for (path, frame_size, bitrate, capacity) in [
        ("/tmp/opus-cold-entry-1-960-16.f32", 320, 48_000, 159),
        ("/tmp/opus-cold-entry-3-480-4.f32", 160, 25_600, 39),
    ] {
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(bytes.len(), frame_size * 2 * 4);
        let input: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&x| f32::from_le_bytes(x))
            .collect();
        let config = EncoderConfig {
            sample_rate: SampleRate::Hz16000,
            channels: crate::Channels::Stereo,
            frame_size,
            bitrate,
        };
        let mut encoder = SilkEncoder::new(config).unwrap();
        let mut range = RangeEncoder::new(capacity).unwrap();
        assert_eq!(range.tell(), 1);
        encoder.encode_prefix_ranked(&input, &mut range).unwrap();
        let bits = range.tell();
        let payload = range.finish().unwrap();
        let mut range = crate::range::RangeDecoder::new(&payload).unwrap();
        let mut decoder =
            SilkDecoder::new(config.sample_rate, config.channels, (frame_size / 16) as u8).unwrap();
        decoder.begin_packet();
        decoder.decode_header_bits(&mut range, true).unwrap();
        let mut output = vec![0_i16; frame_size * 2];
        assert_eq!(
            decoder
                .decode_silk_frame(&mut range, true, &mut output)
                .unwrap(),
            frame_size
        );
        let mut signal = 0.0_f64;
        let mut error = 0.0_f64;
        // Same-rate SILK includes stereo's one sample plus resampler delay.
        for (original, reconstructed) in input
            .as_chunks::<2>()
            .0
            .iter()
            .zip(output.as_chunks::<2>().0.iter().skip(1))
        {
            for channel in 0..2 {
                let x = f64::from(original[channel]);
                let y = f64::from(reconstructed[channel]) / 32768.0;
                signal = x.mul_add(x, signal);
                error += (x - y).powi(2);
            }
        }
        eprintln!(
            "cold prefix path={path} bits={bits} gains={:?} indices={:?} snr={}",
            encoder.gain,
            encoder.previous_index,
            10.0 * (signal / error).log10()
        );
    }
}
