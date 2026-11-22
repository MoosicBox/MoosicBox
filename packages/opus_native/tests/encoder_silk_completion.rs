//! Standalone SILK completion: persistent speech coding and explicit C oracle.
#![cfg(all(feature = "silk", feature = "celt"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
use moosicbox_opus_native::{Channels, Decoder, Encoder, EncoderConfig, SampleRate};
use std::io::Write;
use std::process::{Command, Stdio};

fn speech(rate: usize, count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let phase = std::f32::consts::TAU * 125.0 * t;
            let envelope = 0.6 + 0.4 * (std::f32::consts::TAU * 3.0 * t).sin();
            envelope
                * (0.18 * phase.sin() + 0.08 * (2.0 * phase).sin() + 0.04 * (5.0 * phase).sin())
        })
        .collect()
}

fn encode(rate: usize, milliseconds: usize, bitrate: u32) -> (Vec<Vec<u8>>, Vec<i16>) {
    let config = EncoderConfig {
        sample_rate: SampleRate::from_hz(rate as u32).unwrap(),
        channels: Channels::Mono,
        frame_size: rate * milliseconds / 1000,
        bitrate,
    };
    let mut encoder = Encoder::new_silk(config).unwrap();
    let mut decoder = Decoder::new(config.sample_rate, Channels::Mono).unwrap();
    let mut packets = Vec::new();
    let mut decoded = Vec::new();
    let input = speech(rate, config.frame_size * 12);
    for frame in input.chunks_exact(config.frame_size) {
        let packet = encoder.encode(frame).unwrap();
        assert!(packet.len() * 8000 <= bitrate as usize * milliseconds);
        let mut output = vec![0_i16; config.frame_size];
        assert_eq!(
            decoder.decode(Some(&packet), &mut output, false).unwrap(),
            config.frame_size
        );
        decoded.extend(output);
        packets.push(packet);
    }
    let energy = decoded
        .iter()
        .map(|&v| i64::from(v) * i64::from(v))
        .sum::<i64>();
    assert!(energy > decoded.len() as i64 * 10000);
    let source: Vec<_> = input.iter().map(|v| f64::from(*v) * 32768.0).collect();
    // Use the public encoder's declared reference output-conversion delay.
    // Do not fit amplitude or alter frozen-corpus expectations.
    let signal = source.iter().map(|v| v * v).sum::<f64>();
    let error = decoded
        .iter()
        .skip(encoder.lookahead())
        .zip(&source)
        .map(|(&v, x)| (f64::from(v) - x).powi(2))
        .sum::<f64>();
    let snr = 10.0 * (signal / error).log10();
    assert!(
        snr > 8.0,
        "rate={rate} ms={milliseconds} bitrate={bitrate}: SNR={snr}"
    );
    (packets, decoded)
}

#[test]
fn speech_budget_and_history() {
    for rate in [8000, 12000, 16000] {
        for duration in [10, 20, 40, 60] {
            encode(rate, duration, 48000);
        }
    }
}

#[test]
#[ignore = "requires explicit pinned MOOSICBOX_OPUS_REFERENCE_ORACLE executable"]
fn pinned_c_decodes_native_voiced_speech() {
    let executable = std::env::var("MOOSICBOX_OPUS_REFERENCE_ORACLE")
        .expect("explicit pinned oracle executable required");
    let provenance = Command::new(&executable)
        .arg("provenance")
        .output()
        .unwrap();
    assert!(
        provenance.status.success(),
        "oracle provenance verification failed"
    );
    for rate in [8000, 12000, 16000] {
        for duration in [10, 20, 40, 60] {
            let (packets, native) = encode(rate, duration, 48000);
            let mut request = format!("{{\"op\":\"decoder\",\"rate\":{rate},\"channels\":1}}\n");
            for packet in &packets {
                request.push_str(&format!(
                    "{{\"op\":\"decode\",\"frame_size\":{},\"packet\":{packet:?}}}\n",
                    rate * duration / 1000
                ));
            }
            // Feed through a dedicated thread to avoid pipe backpressure for
            // persistent multi-frame responses larger than a platform pipe.
            let mut child = Command::new(&executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let mut stdin = child.stdin.take().unwrap();
            let writer = std::thread::spawn(move || stdin.write_all(request.as_bytes()).unwrap());
            let result = child.wait_with_output().unwrap();
            writer.join().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let response = String::from_utf8(result.stdout).unwrap();
            let mut reference = Vec::new();
            for line in response.lines().skip(1) {
                let begin = line.find("\"pcm\":[").expect("PCM response") + 7;
                let end = line[begin..].find(']').unwrap() + begin;
                reference.extend(
                    line[begin..end]
                        .split(',')
                        .map(|value| value.trim().parse::<i16>().unwrap()),
                );
            }
            assert_eq!(reference.len(), native.len());
            let maximum = reference
                .iter()
                .zip(&native)
                .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            assert!(
                maximum <= 2,
                "native/C voiced mismatch at {rate} Hz: max={maximum}"
            );
        }
    }
}
