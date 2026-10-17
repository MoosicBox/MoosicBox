#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub use moosicbox_opus_native::{Error, Result};
#[path = "../src/output.rs"]
mod output;

use output::CeltOutput;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn pinned_reference_deemphasis_matches_all_rates_channels_and_shapes() {
    for channels in [1, 2] {
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for shape in 0..3 {
                let signal = (0..240 * channels)
                    .map(|index| match shape {
                        0 => {
                            if index < channels {
                                1 << 24
                            } else {
                                0
                            }
                        }
                        1 => {
                            if index % 2 == 0 {
                                13_777_111
                            } else {
                                -13_777_111
                            }
                        }
                        _ => {
                            if (index / channels) % 2 == 0 {
                                536_870_911
                            } else {
                                -536_870_911
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let mut child = Command::new("python3")
                    .arg(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/output_oracle.py"
                    ))
                    .args([channels.to_string(), rate.to_string()])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(
                        &signal
                            .iter()
                            .flat_map(|value: &i32| value.to_le_bytes())
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
                let result = child.wait_with_output().unwrap();
                assert!(result.status.success(), "reference oracle must execute");
                let expected = result
                    .stdout
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
                    .collect::<Vec<_>>();
                let mut output = vec![0; expected.len()];
                let count = CeltOutput::default()
                    .process(&signal, 240, channels, rate, &mut output)
                    .unwrap();
                assert_eq!(count, 240 * rate as usize / 48000);
                assert_eq!(
                    output, expected,
                    "channels={channels} rate={rate} shape={shape}"
                );
                let mut state = CeltOutput::default();
                let mut chunked = vec![0; expected.len()];
                let chunk_samples = 120 * rate as usize / 48000 * channels;
                for (index, chunk) in signal.chunks_exact(120 * channels).enumerate() {
                    state
                        .process(
                            chunk,
                            120,
                            channels,
                            rate,
                            &mut chunked[index * chunk_samples..][..chunk_samples],
                        )
                        .unwrap();
                }
                assert_eq!(chunked, expected, "persistent state across chunk boundary");
            }
        }
    }
}

#[test]
fn malformed_shape_and_short_buffer_do_not_mutate_state_or_output() {
    let mut state = CeltOutput::default();
    let mut output = [123; 40];
    for (signal, frames, channels) in [
        (vec![1; 120], 120, 2),
        (vec![1; 239], 120, 2),
        (vec![1; 241], 120, 2),
        (vec![1; 119], 119, 1),
    ] {
        assert!(
            state
                .process(&signal, frames, channels, 8000, &mut output)
                .is_err()
        );
        assert_eq!(output, [123; 40]);
    }
    assert!(
        state
            .process(&[1; 120], 120, 2, 48000, &mut output)
            .is_err()
    );
    let mut actual = [0; 20];
    state.process(&[0; 120], 120, 1, 8000, &mut actual).unwrap();
    assert_eq!(actual, [0; 20]);
}

#[cfg(feature = "celt")]
#[test]
fn public_silence_packet_counts_and_reset_match_all_rates_and_channels() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let mut decoder = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
            let mut pcm = vec![1; rate as usize / 100 * channels as usize];
            // CELT 10 ms FB silence, one frame. Stereo flag follows output channels.
            let packet = [
                if channels == Channels::Mono {
                    0xf0
                } else {
                    0xf4
                },
                0xff,
                0xfe,
            ];
            let count = decoder.decode(Some(&packet), &mut pcm, false).unwrap();
            assert_eq!(count, rate as usize / 100);
            assert!(pcm.iter().all(|sample| *sample == 0));
            decoder.reset_state().unwrap();
            assert_eq!(
                decoder.decode(Some(&packet), &mut pcm, false).unwrap(),
                count
            );
        }
    }
}

#[test]
fn output_durations_and_reset_state_are_exact() {
    for frames in [120, 240, 480, 960] {
        for channels in [1, 2] {
            for rate in [8000, 12000, 16000, 24000, 48000] {
                let mut state = CeltOutput::default();
                let signal = vec![4096; frames * channels];
                let mut pcm = vec![0; frames * rate as usize / 48000 * channels];
                assert_eq!(
                    state
                        .process(&signal, frames, channels, rate, &mut pcm)
                        .unwrap(),
                    frames * rate as usize / 48000
                );
                assert!(pcm.iter().any(|sample| *sample != 0));
                state = CeltOutput::default();
                state
                    .process(
                        &vec![0; frames * channels],
                        frames,
                        channels,
                        rate,
                        &mut pcm,
                    )
                    .unwrap();
                assert!(pcm.iter().all(|sample| *sample == 0));
            }
        }
    }
}

#[cfg(all(feature = "silk", feature = "celt"))]
fn mono_packet_output_oracle(rate_filter: u32) {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for (rate, paths) in [
        (
            8000,
            vec![root.join("src/silk/reference/nb_sine_mono.packet"); 2],
        ),
        (
            48000,
            vec![
                root.join("test_vectors/frozen/celt_fb_mono/packet.bin"),
                root.join("test_vectors/frozen/celt_fb_mono/packet_001/packet.bin"),
            ],
        ),
    ] {
        if rate != rate_filter {
            continue;
        }
        let packets = paths
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect::<Vec<_>>();
        let mut actual_by_channels = Vec::new();
        let mut mismatches = Vec::new();
        for channels in [Channels::Mono, Channels::Stereo] {
            let mut child = Command::new("python3")
                .arg(root.join("tests/output_oracle.py"))
                .args([
                    "--decode".into(),
                    rate.to_string(),
                    (channels as usize).to_string(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            for packet in &packets {
                input
                    .write_all(&u32::try_from(packet.len()).unwrap().to_le_bytes())
                    .unwrap();
                input.write_all(packet).unwrap();
            }
            drop(input);
            let reference = child.wait_with_output().unwrap();
            assert!(reference.status.success());
            let expected = reference
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|bytes| i16::from_le_bytes(*bytes))
                .collect::<Vec<_>>();
            let mut decoder = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
            let mut actual = Vec::new();
            let mut first = Vec::new();
            for packet in &packets {
                let mut pcm = vec![0; 5760 * channels as usize];
                let count = decoder.decode(Some(packet), &mut pcm, false).unwrap();
                pcm.truncate(count * channels as usize);
                if first.is_empty() {
                    first.clone_from(&pcm);
                }
                actual.extend_from_slice(&pcm);
            }
            assert!(
                actual.iter().any(|sample| *sample != 0),
                "nonzero oracle required"
            );
            let first_difference = actual.iter().zip(&expected).position(|(a, b)| a != b);
            if actual != expected {
                mismatches.push(format!("rate={rate}, channels={channels:?}, lengths={}/{}, first_difference={first_difference:?}", actual.len(), expected.len()));
            }
            decoder.reset_state().unwrap();
            let mut pcm = vec![0; 5760 * channels as usize];
            let count = decoder.decode(Some(&packets[0]), &mut pcm, false).unwrap();
            assert_eq!(&pcm[..count * channels as usize], first);
            actual_by_channels.push(actual);
        }
        assert_eq!(
            actual_by_channels[1],
            actual_by_channels[0]
                .iter()
                .flat_map(|sample| [*sample, *sample])
                .collect::<Vec<_>>()
        );
        assert!(
            mismatches.is_empty(),
            "public C API mismatches: {}",
            mismatches.join("; ")
        );
    }
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
fn silk_mono_to_stereo_exact_public_api() {
    mono_packet_output_oracle(8000);
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
fn celt_mono_to_stereo_exact_public_api() {
    mono_packet_output_oracle(48000);
}

#[cfg(feature = "silk")]
#[test]
fn silk_stereo_coded_output_mapping_matches_pinned_api_all_rates() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut failures = Vec::new();
    for name in ["mb_mixed_stereo.packet", "wb_mixed.packet"] {
        let packet = std::fs::read(root.join("src/silk/reference").join(name)).unwrap();
        assert_ne!(packet[0] & 4, 0, "oracle must be stereo coded");
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for channels in [Channels::Mono, Channels::Stereo] {
                let mut child = Command::new("python3")
                    .arg(root.join("tests/output_oracle.py"))
                    .args([
                        "--decode".into(),
                        rate.to_string(),
                        (channels as usize).to_string(),
                    ])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap();
                let mut input = child.stdin.take().unwrap();
                for _ in 0..2 {
                    input
                        .write_all(&u32::try_from(packet.len()).unwrap().to_le_bytes())
                        .unwrap();
                    input.write_all(&packet).unwrap();
                }
                drop(input);
                let reference = child.wait_with_output().unwrap();
                assert!(reference.status.success());
                let expected = reference
                    .stdout
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|sample| i16::from_le_bytes(*sample))
                    .collect::<Vec<_>>();
                let mut decoder =
                    Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                let mut actual = Vec::new();
                let mut first = Vec::new();
                for index in 0..2 {
                    let mut output = vec![0; 5760 * channels as usize];
                    let count = decoder.decode(Some(&packet), &mut output, false).unwrap();
                    assert_eq!(count, rate as usize / 50, "20ms duration");
                    output.truncate(count * channels as usize);
                    if index == 0 {
                        first.clone_from(&output);
                    }
                    actual.extend(output);
                }
                assert!(expected.iter().any(|sample| *sample != 0));
                if actual != expected {
                    failures.push(format!(
                        "{name} rate={rate} channels={channels:?}: first={:?}, lengths={}/{}",
                        actual.iter().zip(&expected).position(|(a, b)| a != b),
                        actual.len(),
                        expected.len()
                    ));
                }
                decoder.reset_state().unwrap();
                let mut output = vec![0; first.len()];
                decoder.decode(Some(&packet), &mut output, false).unwrap();
                assert_eq!(output, first, "reset must reproduce first packet");
            }
        }
    }
    assert!(
        failures.is_empty(),
        "actual C API mapping mismatches: {}",
        failures.join("; ")
    );
}

#[cfg(feature = "celt")]
#[test]
fn celt_stereo_coded_output_mapping_matches_pinned_api_all_rates() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = root.join("test_vectors/frozen/celt_fb_stereo");
    let packets = [".", "packet_001", "packet_002", "packet_003"]
        .iter()
        .map(|path| std::fs::read(base.join(path).join("packet.bin")).unwrap())
        .collect::<Vec<_>>();
    let mut failures = Vec::new();
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let mut child = Command::new("python3")
                .arg(root.join("tests/output_oracle.py"))
                .args([
                    "--decode".into(),
                    rate.to_string(),
                    (channels as usize).to_string(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            for packet in &packets {
                input
                    .write_all(&u32::try_from(packet.len()).unwrap().to_le_bytes())
                    .unwrap();
                input.write_all(packet).unwrap();
            }
            drop(input);
            let reference = child.wait_with_output().unwrap();
            assert!(reference.status.success());
            let expected = reference
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|sample| i16::from_le_bytes(*sample))
                .collect::<Vec<_>>();
            let mut decoder = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
            let mut actual = Vec::new();
            let mut first = Vec::new();
            for (index, packet) in packets.iter().enumerate() {
                let mut output = vec![0; 5760 * channels as usize];
                let count = decoder.decode(Some(packet), &mut output, false).unwrap();
                assert_eq!(count, rate as usize / 50);
                output.truncate(count * channels as usize);
                if index == 0 {
                    first.clone_from(&output);
                }
                actual.extend(output);
            }
            assert!(expected.iter().any(|sample| *sample != 0));
            if actual != expected {
                failures.push(format!(
                    "rate={rate} channels={channels:?} first={:?} lengths={}/{}",
                    actual.iter().zip(&expected).position(|(a, b)| a != b),
                    actual.len(),
                    expected.len()
                ));
            }
            decoder.reset_state().unwrap();
            let mut output = vec![0; first.len()];
            decoder
                .decode(Some(&packets[0]), &mut output, false)
                .unwrap();
            assert_eq!(output, first);
        }
    }
    assert!(
        failures.is_empty(),
        "CELT public C API mismatch: {}",
        failures.join("; ")
    );
}

#[cfg(feature = "silk")]
#[test]
fn unsupported_fec_and_plc_preserve_output_and_decoder_state() {
    use moosicbox_opus_native::{Channels, Decoder, Error, SampleRate};
    let packet = include_bytes!("../src/silk/reference/nb_sine_mono.packet");
    let mut decoder = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
    let mut control = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
    decoder.decode(Some(packet), &mut [0; 160], false).unwrap();
    control.decode(Some(packet), &mut [0; 160], false).unwrap();
    for (input, fec) in [(Some(packet.as_slice()), true), (None, false), (None, true)] {
        let mut output = [123; 160];
        assert!(matches!(
            decoder.decode(input, &mut output, fec),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(output, [123; 160]);
        let mut float = [0.25; 160];
        assert!(matches!(
            decoder.decode_float(input, &mut float, fec),
            Err(Error::Unsupported(_))
        ));
        assert!(
            float
                .iter()
                .all(|value| value.to_bits() == 0.25_f32.to_bits())
        );
    }
    let mut actual = [0; 160];
    let mut expected = [0; 160];
    decoder.decode(Some(packet), &mut actual, false).unwrap();
    control.decode(Some(packet), &mut expected, false).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn non_res24_reference_rounding_boundary_is_preserved() {
    let mut output = [0; 1];
    CeltOutput::default()
        .process(&[173 * 4096 + 2040], 1, 1, 48000, &mut output)
        .unwrap();
    assert_eq!(output, [173]);
}

#[test]
fn storage_shrink_matches_upstream_shared_entropy_exactly() {
    use moosicbox_opus_native::range::RangeDecoder;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let reference = Command::new("python3")
        .arg(root.join("tests/output_oracle.py"))
        .arg("--shrink")
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let expected = String::from_utf8(reference.stdout)
        .unwrap()
        .split_whitespace()
        .map(|word| word.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    let data = (0..32)
        .map(|index| u8::try_from((index * 73 + 19) & 255).unwrap())
        .collect::<Vec<_>>();
    let mut actual = Vec::new();
    for buffered in [false, true] {
        let mut ec = RangeDecoder::new(&data).unwrap();
        for index in 0..17 {
            actual.push(u32::from(ec.ec_dec_bit_logp(1 + index % 4).unwrap()));
        }
        if buffered {
            actual.push(ec.ec_dec_bits(5).unwrap());
        }
        let before = (ec.ec_tell(), ec.get_range());
        assert!(ec.shrink_storage(33).is_err());
        ec.shrink_storage(25).unwrap();
        assert_eq!(before, (ec.ec_tell(), ec.get_range()));
        actual.extend([ec.ec_tell(), ec.get_range()]);
        for index in 0..6 {
            actual.push(ec.ec_dec_bits(3 + index).unwrap());
            actual.push(ec.ec_dec_uint(37).unwrap());
            actual.extend([ec.ec_tell(), ec.get_range()]);
        }
        ec.shrink_storage(0).unwrap();
    }
    assert_eq!(actual, expected);
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
fn hybrid_nonzero_persistent_mapping_matches_pinned_api_all_rates() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = root.join("test_vectors/frozen/hybrid_fb_mono");
    let packets = [".", "packet_001", "packet_002", "packet_003"]
        .iter()
        .map(|path| std::fs::read(base.join(path).join("packet.bin")).unwrap())
        .collect::<Vec<_>>();
    let mut failures = Vec::new();
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let mut child = Command::new("python3")
                .arg(root.join("tests/output_oracle.py"))
                .args([
                    "--decode".into(),
                    rate.to_string(),
                    (channels as usize).to_string(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            for packet in &packets {
                input
                    .write_all(&u32::try_from(packet.len()).unwrap().to_le_bytes())
                    .unwrap();
                input.write_all(packet).unwrap();
            }
            drop(input);
            let reference = child.wait_with_output().unwrap();
            assert!(reference.status.success());
            let expected = reference
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|sample| i16::from_le_bytes(*sample))
                .collect::<Vec<_>>();
            let mut decoder = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
            let mut actual = Vec::new();
            let mut first = Vec::new();
            for (index, packet) in packets.iter().enumerate() {
                let mut output = vec![0; 5760 * channels as usize];
                let count = decoder.decode(Some(packet), &mut output, false).unwrap();
                assert_eq!(count, rate as usize / 50);
                output.truncate(count * channels as usize);
                if index == 0 {
                    first.clone_from(&output);
                }
                actual.extend(output);
            }
            assert!(expected.iter().any(|sample| *sample != 0));
            if actual != expected {
                failures.push(format!(
                    "rate={rate} channels={channels:?} first={:?} lengths={}/{}",
                    actual.iter().zip(&expected).position(|(a, b)| a != b),
                    actual.len(),
                    expected.len()
                ));
            }
            decoder.reset_state().unwrap();
            let mut output = vec![0; first.len()];
            decoder
                .decode(Some(&packets[0]), &mut output, false)
                .unwrap();
            assert_eq!(output, first);
        }
    }
    assert!(
        failures.is_empty(),
        "Hybrid public C API mismatch: {}",
        failures.join("; ")
    );
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
fn hybrid_stereo_nonzero_persistent_mapping_matches_pinned_api_all_rates() {
    use moosicbox_opus_native::{Channels, Decoder, SampleRate};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = root.join("test_vectors/frozen/corpus/sequences/hybrid_swb_stereo");
    let packets = [".", "packet_001", "packet_002", "packet_003"]
        .iter()
        .map(|path| std::fs::read(base.join(path).join("packet.bin")).unwrap())
        .collect::<Vec<_>>();
    let mut failures = Vec::new();
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let mut child = Command::new("python3")
                .arg(root.join("tests/output_oracle.py"))
                .args([
                    "--decode".into(),
                    rate.to_string(),
                    (channels as usize).to_string(),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            for packet in &packets {
                input
                    .write_all(&u32::try_from(packet.len()).unwrap().to_le_bytes())
                    .unwrap();
                input.write_all(packet).unwrap();
            }
            drop(input);
            let reference = child.wait_with_output().unwrap();
            assert!(reference.status.success());
            let expected = reference
                .stdout
                .as_chunks::<2>()
                .0
                .iter()
                .map(|sample| i16::from_le_bytes(*sample))
                .collect::<Vec<_>>();
            let mut decoder = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
            let mut actual = Vec::new();
            let mut first = Vec::new();
            for (index, packet) in packets.iter().enumerate() {
                let mut output = vec![0; 5760 * channels as usize];
                let count = decoder.decode(Some(packet), &mut output, false).unwrap();
                assert_eq!(count, rate as usize / 50);
                output.truncate(count * channels as usize);
                if index == 0 {
                    first.clone_from(&output);
                }
                actual.extend(output);
            }
            assert!(expected.iter().any(|sample| *sample != 0));
            if actual != expected {
                failures.push(format!(
                    "rate={rate} channels={channels:?} first={:?} lengths={}/{}",
                    actual.iter().zip(&expected).position(|(a, b)| a != b),
                    actual.len(),
                    expected.len()
                ));
            }
            decoder.reset_state().unwrap();
            let mut output = vec![0; first.len()];
            decoder
                .decode(Some(&packets[0]), &mut output, false)
                .unwrap();
            assert_eq!(output, first);
        }
    }
    assert!(
        failures.is_empty(),
        "Hybrid public C API mismatch: {}",
        failures.join("; ")
    );
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
fn hybrid_unverified_transitions_are_explicit_and_preserve_output() {
    use moosicbox_opus_native::{Channels, Decoder, Error, SampleRate};
    let mono = include_bytes!("../test_vectors/frozen/hybrid_fb_mono/packet.bin");
    let stereo =
        include_bytes!("../test_vectors/frozen/corpus/sequences/hybrid_swb_stereo/packet.bin");
    let celt = include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin");
    let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    decoder.decode(Some(mono), &mut [0; 960], false).unwrap();
    for packet in [stereo.as_slice(), celt.as_slice()] {
        let mut output = [123; 960];
        assert!(matches!(
            decoder.decode(Some(packet), &mut output, false),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(output, [123; 960]);
    }
    decoder.reset_state().unwrap();
    decoder.decode(Some(celt), &mut [0; 960], false).unwrap();
    let mut output = [123; 960];
    assert!(matches!(
        decoder.decode(Some(mono), &mut output, false),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(output, [123; 960]);
}
