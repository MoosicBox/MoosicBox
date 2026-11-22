//! Explicit test-only comparison against the pinned C executable.
use super::*;
use std::fmt::Write as _;
use std::io::Write;
use std::process::{Command, Stdio};

pub(super) fn compare_reference(config: EncoderConfig, packets: &[Vec<u8>], native: &[i16]) {
    let executable = std::env::var("MOOSICBOX_OPUS_REFERENCE_ORACLE")
        .expect("explicit pinned C oracle required");
    assert!(
        Command::new(&executable)
            .arg("provenance")
            .output()
            .unwrap()
            .status
            .success()
    );
    let channels = super::super::channel_count(config.channels);
    let mut request = format!(
        "{{\"op\":\"decoder\",\"rate\":{},\"channels\":{channels}}}\n",
        config.sample_rate as u32
    );
    for packet in packets {
        writeln!(
            request,
            "{{\"op\":\"decode\",\"frame_size\":{},\"packet\":{packet:?}}}",
            config.frame_size
        )
        .unwrap();
    }
    let mut child = Command::new(executable)
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
        .zip(native)
        .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
        .max()
        .unwrap();
    assert!(
        maximum <= 2,
        "low-rate native/C mismatch config={config:?} max={maximum}"
    );
}

fn noisy_frame(
    frame_size: usize,
    rate: usize,
    channels: usize,
    kind: usize,
    frame: usize,
    random: &mut u32,
) -> Vec<f32> {
    let mut pcm = Vec::new();
    for i in 0..frame_size {
        for channel in 0..channels {
            *random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (*random as i32 as f32) / 2_147_483_648.0;
            let frequency_offset = channel as f32 * 43.0;
            let phase = std::f32::consts::TAU
                * (125.0 + frequency_offset)
                * (frame * frame_size + i) as f32
                / rate as f32;
            pcm.push(match kind {
                0 => 0.18 * noise,
                1 => {
                    let tone = 0.18 * phase.sin();
                    let background = 0.03 * noise;
                    tone + background
                }
                _ => {
                    if (i + frame * frame_size).is_multiple_of(997) {
                        0.9
                    } else {
                        0.01 * noise
                    }
                }
            });
        }
    }
    pcm
}

// Independent configurations can run concurrently under libtest.
#[test_case::test_case(8000, 1)]
#[test_case::test_case(8000, 2)]
#[test_case::test_case(12000, 1)]
#[test_case::test_case(12000, 2)]
#[test_case::test_case(16000, 1)]
#[test_case::test_case(16000, 2)]
fn noisy_low_rate_mono_stereo_policy(rate: usize, channels: usize) {
    let mut failures = Vec::new();
    for milliseconds in [10, 20, 40, 60] {
        for per_channel in [12000, 16000, 24000] {
            for kind in 0..3 {
                let config = EncoderConfig {
                    sample_rate: SampleRate::from_hz(rate as u32).unwrap(),
                    channels: if channels == 1 {
                        crate::Channels::Mono
                    } else {
                        crate::Channels::Stereo
                    },
                    frame_size: rate * milliseconds / 1000,
                    bitrate: per_channel * channels as u32,
                };
                let mut encoder = SilkEncoder::new(config).unwrap();
                let mut decoder = crate::Decoder::new(config.sample_rate, config.channels).unwrap();
                let mut input = Vec::new();
                let mut reconstructed = Vec::new();
                let mut random = 19_u32;
                let mut bytes = 0;
                let mut errors = 0;
                for frame in 0..8 {
                    let pcm =
                        noisy_frame(config.frame_size, rate, channels, kind, frame, &mut random);
                    input.extend_from_slice(&pcm);
                    if let Ok(packet) = encoder.encode(&pcm) {
                        assert!(packet.len() * 8000 <= config.bitrate as usize * milliseconds);
                        bytes += packet.len();
                        let mut output = vec![0; config.frame_size * channels];
                        assert_eq!(
                            decoder.decode(Some(&packet), &mut output, false).unwrap(),
                            config.frame_size
                        );
                        reconstructed.extend(output);
                    } else {
                        errors += 1;
                        break;
                    }
                }
                let delay = (match rate {
                    8000 => 5,
                    12000 => 10,
                    _ => 13,
                }) * channels;
                let signal = input
                    .iter()
                    .take(reconstructed.len().saturating_sub(delay))
                    .map(|&x| f64::from(x).powi(2))
                    .sum::<f64>();
                let error = reconstructed
                    .iter()
                    .skip(delay)
                    .zip(&input)
                    .map(|(&a, &b)| (f64::from(a) / 32768.0 - f64::from(b)).powi(2))
                    .sum::<f64>();
                let snr = 10.0 * (signal / error).log10();
                let clips = reconstructed
                    .iter()
                    .filter(|&&x| x == i16::MIN || x == i16::MAX)
                    .count();
                eprintln!(
                    "noisy rate={rate} ch={channels} ms={milliseconds} bitrate={} kind={kind} bytes={bytes} snr={snr:.3} clips={clips} errors={errors}",
                    config.bitrate
                );
                if errors != 0
                    || !snr.is_finite()
                    || snr < if kind == 1 { 5.0 } else { 0.0 }
                    || clips != 0
                {
                    failures.push((rate, channels, milliseconds, per_channel, kind));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "unqualified noisy policy cases: {failures:?}"
    );
}
