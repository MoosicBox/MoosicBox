//! Live public-libopus comparison for persistent decoder configuration changes.

#![cfg(feature = "silk")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use std::{path::Path, process::Command};

fn word(bytes: &[u8], offset: &mut usize) -> usize {
    let value = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    usize::try_from(value).unwrap()
}

fn has_lbrr(packet: &[u8], ms: usize) -> bool {
    let toc = moosicbox_opus_native::Toc::parse(packet[0]);
    let frames = if ms <= 20 { 1 } else { ms / 20 };
    let payloads = moosicbox_opus_native::framing::parse_frames(packet).unwrap();
    let mut range = moosicbox_opus_native::range::RangeDecoder::new(payloads[0]).unwrap();
    let mut redundant = false;
    for _ in 0..toc.channels() as usize {
        for _ in 0..frames {
            range.ec_dec_bit_logp(1).unwrap();
        }
        redundant |= range.ec_dec_bit_logp(1).unwrap();
    }
    redundant
}

#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn persistent_silk_duration_changes_match_public_c_decoder() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (rate, rate_changes, recover, long) in [
        (8000, false, false, false),
        (12000, false, false, false),
        (16000, false, false, false),
        (16000, true, false, false),
        (48000, true, false, false),
        (48000, false, false, false),
        (8000, false, true, false),
        (12000, false, true, false),
        (16000, false, true, false),
        (16000, false, false, true),
    ] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let mut command = Command::new("python3");
            command.arg("-B");
            command
                .arg(root.join("src/silk/duration_oracle.py"))
                .arg(rate.to_string())
                .arg((channels as usize).to_string());
            if rate_changes {
                command.arg("--rates");
            }
            if long {
                command.arg("--fec-long");
            }
            if recover {
                command.arg("--recover");
            }
            if rate == 8000 {
                command.arg("--fec-packets");
            }
            if !rate_changes && rate == 48000 && channels == Channels::Stereo {
                command.arg("--channels");
            }
            let oracle = command.output().expect("run pinned public C oracle");
            assert!(
                oracle.status.success(),
                "{}",
                String::from_utf8_lossy(&oracle.stderr)
            );
            let bytes = &oracle.stdout;
            let mut offset = 0;
            let mut reset = usize::MAX;
            let mut decoder = Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
            let mut records = 0;
            let mut redundant_packets = 0;
            while offset < bytes.len() {
                let next_reset = word(bytes, &mut offset);
                let ms = word(bytes, &mut offset);
                let length = word(bytes, &mut offset);
                let count = word(bytes, &mut offset);
                if next_reset != reset {
                    decoder.reset_state().unwrap();
                    reset = next_reset;
                }
                let packet = &bytes[offset..offset + length];
                if rate == 8000 {
                    redundant_packets += usize::from(has_lbrr(packet, ms));
                }
                offset += length;
                let sample_count = count * channels as usize;
                let expected = bytes[offset..offset + 2 * sample_count]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|x| i16::from_le_bytes([x[0], x[1]]))
                    .collect::<Vec<_>>();
                offset += 2 * sample_count;
                let mut actual = vec![0; sample_count];
                assert_eq!(
                    decoder
                        .decode(Some(packet), &mut actual, recover && records % 10 == 3)
                        .unwrap(),
                    count
                );
                assert!(expected.iter().any(|&x| x != 0), "nonzero oracle signal");
                let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                assert!(
                    first.is_none(),
                    "rate={rate} channels={channels:?} reset={reset} record={records} ms={ms} first divergence={first:?}"
                );
                records += 1;
            }
            assert_eq!(records, 20);
            if rate == 8000 {
                assert!(
                    redundant_packets > 0,
                    "oracle must actually exercise LBRR-bearing packets"
                );
            }
        }
    }
}

#[test]
fn explicit_loss_duration_validates_without_touching_output() {
    let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
    let mut output = vec![47; 1920];
    for size in [0, 1, 119, 121, 5880] {
        assert!(matches!(
            decoder.decode_loss(&mut output, size),
            Err(moosicbox_opus_native::Error::InvalidPacket(_))
        ));
        assert!(output.iter().all(|&sample| sample == 47));
    }
    assert_eq!(decoder.decode_loss(&mut output, 960).unwrap(), 960);
    assert!(output.iter().all(|&sample| sample == 0));
}

#[cfg(feature = "celt")]
#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn public_celt_loss_bursts_and_recovery_match_pinned_api() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = root.parent().unwrap().parent().unwrap();
    for coded in [1, 2] {
        for channels in [Channels::Mono, Channels::Stereo] {
            for size in [120, 240, 480, 960] {
                for burst in [1, 7, 45] {
                    let oracle = Command::new("python3")
                        .arg("-B")
                        .arg(root.join("src/silk/celt_packet_loss_oracle.py"))
                        .arg(repository)
                        .arg(coded.to_string())
                        .arg(size.to_string())
                        .arg(burst.to_string())
                        .arg((channels as usize).to_string())
                        .output()
                        .unwrap();
                    assert!(
                        oracle.status.success(),
                        "{}",
                        String::from_utf8_lossy(&oracle.stderr)
                    );
                    let mut decoder = Decoder::new(SampleRate::Hz48000, channels).unwrap();
                    let mut offset = 0;
                    for step in 0..2 * (8 + burst) {
                        if step == 8 + burst {
                            decoder.reset_state().unwrap();
                        }
                        let length = word(&oracle.stdout, &mut offset);
                        let count = word(&oracle.stdout, &mut offset);
                        let packet = &oracle.stdout[offset..offset + length];
                        offset += length;
                        let expected = oracle.stdout[offset..offset + count * 2]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|x| i16::from_le_bytes(*x))
                            .collect::<Vec<_>>();
                        offset += count * 2;
                        let mut actual = vec![0; count];
                        let produced = if length == 0 {
                            if step % 2 == 0 {
                                decoder.decode_loss(&mut actual, size)
                            } else {
                                decoder.decode(None, &mut actual, false)
                            }
                        } else {
                            decoder.decode(Some(packet), &mut actual, false)
                        }
                        .unwrap();
                        assert_eq!(produced, size);
                        let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                        assert!(
                            first.is_none(),
                            "coded={coded} channels={channels:?} size={size} burst={burst} step={step} divergence={first:?}"
                        );
                    }
                    assert_eq!(offset, oracle.stdout.len());
                }
            }
        }
    }
}
#[cfg(feature = "celt")]
#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn public_celt_loss_output_rates_match_pinned_api() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = root.parent().unwrap().parent().unwrap();
    for rate in [8000, 12000, 16000, 24000] {
        for coded in [1, 2] {
            for channels in [Channels::Mono, Channels::Stereo] {
                for size in [480] {
                    for burst in [7] {
                        let oracle = Command::new("python3")
                            .arg("-B")
                            .arg(root.join("src/silk/celt_packet_loss_oracle.py"))
                            .arg(repository)
                            .arg(coded.to_string())
                            .arg(size.to_string())
                            .arg(burst.to_string())
                            .arg((channels as usize).to_string())
                            .arg(rate.to_string())
                            .output()
                            .unwrap();
                        assert!(
                            oracle.status.success(),
                            "{}",
                            String::from_utf8_lossy(&oracle.stderr)
                        );
                        let mut decoder =
                            Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                        let mut offset = 0;
                        for step in 0..2 * (8 + burst) {
                            if step == 8 + burst {
                                decoder.reset_state().unwrap();
                            }
                            let length = word(&oracle.stdout, &mut offset);
                            let count = word(&oracle.stdout, &mut offset);
                            let packet = &oracle.stdout[offset..offset + length];
                            offset += length;
                            let expected = oracle.stdout[offset..offset + count * 2]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|x| i16::from_le_bytes(*x))
                                .collect::<Vec<_>>();
                            offset += count * 2;
                            let mut actual = vec![0; count];
                            let produced = if length == 0 {
                                if step % 2 == 0 {
                                    decoder.decode_loss(&mut actual, size * rate as usize / 48000)
                                } else {
                                    decoder.decode(None, &mut actual, false)
                                }
                            } else {
                                decoder.decode(Some(packet), &mut actual, false)
                            }
                            .unwrap();
                            assert_eq!(produced, size * rate as usize / 48000);
                            let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                            assert!(
                                first.is_none(),
                                "coded={coded} channels={channels:?} size={size} burst={burst} step={step} divergence={first:?}"
                            );
                        }
                        assert_eq!(offset, oracle.stdout.len());
                    }
                }
            }
        }
    }
}
#[cfg(feature = "silk")]
#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn public_silk_loss_prototype_matches_pinned_api() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = root.parent().unwrap().parent().unwrap();
    for bandwidth in [1101, 1102, 1103] {
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for coded in [1, 2] {
                for channels in [Channels::Mono, Channels::Stereo] {
                    for size in [480, 960] {
                        for burst in [1, 7, 45] {
                            let oracle = Command::new("python3")
                                .arg("-B")
                                .arg(root.join("src/silk/celt_packet_loss_oracle.py"))
                                .arg(repository)
                                .arg(coded.to_string())
                                .arg(size.to_string())
                                .arg(burst.to_string())
                                .arg((channels as usize).to_string())
                                .arg(rate.to_string())
                                .arg("silk")
                                .arg(bandwidth.to_string())
                                .output()
                                .unwrap();
                            assert!(
                                oracle.status.success(),
                                "{}",
                                String::from_utf8_lossy(&oracle.stderr)
                            );
                            let mut decoder =
                                Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                            let mut offset = 0;
                            for step in 0..2 * (8 + burst) {
                                if step == 8 + burst {
                                    decoder.reset_state().unwrap();
                                }
                                let length = word(&oracle.stdout, &mut offset);
                                let count = word(&oracle.stdout, &mut offset);
                                let packet = &oracle.stdout[offset..offset + length];
                                offset += length;
                                let expected = oracle.stdout[offset..offset + count * 2]
                                    .as_chunks::<2>()
                                    .0
                                    .iter()
                                    .map(|x| i16::from_le_bytes(*x))
                                    .collect::<Vec<_>>();
                                offset += count * 2;
                                let mut actual = vec![0; count];
                                let produced = if length == 0 {
                                    if step % 2 == 0 {
                                        decoder
                                            .decode_loss(&mut actual, size * rate as usize / 48000)
                                    } else {
                                        decoder.decode(None, &mut actual, false)
                                    }
                                } else {
                                    decoder.decode(Some(packet), &mut actual, false)
                                }
                                .unwrap();
                                assert_eq!(produced, size * rate as usize / 48000);
                                let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                                assert!(
                                    first.is_none(),
                                    "coded={coded} channels={channels:?} size={size} burst={burst} step={step} divergence={first:?}"
                                );
                            }
                            assert_eq!(offset, oracle.stdout.len());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn persistent_mode_transitions_match_public_c() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for abrupt in [false, true] {
        for channels in [Channels::Mono, Channels::Stereo] {
            for rate in [8000, 12000, 16000, 24000, 48000] {
                let mut command = Command::new("python3");
                command
                    .arg("-B")
                    .arg(root.join("src/silk/mode_transition_oracle.py"))
                    .arg((channels as usize).to_string())
                    .arg(rate.to_string());
                if abrupt {
                    command.arg("abrupt");
                }
                let oracle = command.output().unwrap();
                assert!(
                    oracle.status.success(),
                    "{}",
                    String::from_utf8_lossy(&oracle.stderr)
                );
                let mut decoder =
                    Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                let mut offset = 0;
                for step in 0..84 {
                    if step == 42 {
                        decoder.reset_state().unwrap();
                    }
                    let length =
                        u32::from_le_bytes(oracle.stdout[offset..offset + 4].try_into().unwrap())
                            as usize;
                    let count = u32::from_le_bytes(
                        oracle.stdout[offset + 4..offset + 8].try_into().unwrap(),
                    ) as usize;
                    offset += 8;
                    let packet = &oracle.stdout[offset..offset + length];
                    offset += length;
                    let expected = oracle.stdout[offset..offset + count * 2]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|x| i16::from_le_bytes(*x))
                        .collect::<Vec<_>>();
                    offset += count * 2;
                    let mut actual = vec![0; count];
                    decoder.decode(Some(packet), &mut actual, false).unwrap();
                    let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                    assert!(
                        first.is_none(),
                        "channels={channels:?} rate={rate} step={step} toc={} divergence={first:?}",
                        packet[0] >> 3
                    );
                }
                assert_eq!(offset, oracle.stdout.len());
            }
        }
    }
}

#[cfg(feature = "celt")]
#[test]
fn legal_empty_second_constituent_matches_explicit_loss_and_recovery() {
    let mut rejected = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    let mut untouched = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    let mut sentinel = vec![123; 1920];
    assert!(matches!(
        rejected.decode(Some(&[0x7a, 2, 0xff, 0xfe]), &mut sentinel, false),
        Err(moosicbox_opus_native::Error::Unsupported(_))
    ));
    assert!(sentinel.iter().all(|sample| *sample == 123));
    let mut after = vec![0; 960];
    let mut control = vec![0; 960];
    rejected
        .decode(Some(&[0xf8, 0xff, 0xfe]), &mut after, false)
        .unwrap();
    untouched
        .decode(Some(&[0xf8, 0xff, 0xfe]), &mut control, false)
        .unwrap();
    assert_eq!(after, control);
    for channels in [Channels::Mono, Channels::Stereo] {
        let packet = [0xf8, 0xff, 0xfe];
        let combined = [0xfa, 2, 0xff, 0xfe];
        let mut decoder = Decoder::new(SampleRate::Hz48000, channels).unwrap();
        let mut control = Decoder::new(SampleRate::Hz48000, channels).unwrap();
        let width = channels as usize;
        let mut actual = vec![123; 1920 * width];
        assert_eq!(
            decoder.decode(Some(&combined), &mut actual, false).unwrap(),
            1920
        );
        let mut expected = vec![0; 1920 * width];
        control
            .decode(Some(&packet), &mut expected[..960 * width], false)
            .unwrap();
        control
            .decode_loss(&mut expected[960 * width..], 960)
            .unwrap();
        assert_eq!(actual, expected);
        let mut recovered = vec![0; 960 * width];
        let mut reference = vec![0; 960 * width];
        decoder
            .decode(Some(&packet), &mut recovered, false)
            .unwrap();
        control
            .decode(Some(&packet), &mut reference, false)
            .unwrap();
        assert_eq!(recovered, reference);
    }
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn legal_empty_constituent_matches_public_c() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    {
        for channels in [Channels::Mono, Channels::Stereo] {
            for rate in [8000, 12000, 16000, 24000, 48000] {
                let mut command = Command::new("python3");
                command
                    .arg("-B")
                    .arg(root.join("src/silk/empty_frame_oracle.py"))
                    .arg((channels as usize).to_string())
                    .arg(rate.to_string());
                let oracle = command.output().unwrap();
                assert!(
                    oracle.status.success(),
                    "{}",
                    String::from_utf8_lossy(&oracle.stderr)
                );
                let mut decoder =
                    Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                let mut offset = 0;
                for step in 0..8 {
                    let length =
                        u32::from_le_bytes(oracle.stdout[offset..offset + 4].try_into().unwrap())
                            as usize;
                    let count = u32::from_le_bytes(
                        oracle.stdout[offset + 4..offset + 8].try_into().unwrap(),
                    ) as usize;
                    offset += 8;
                    let packet = &oracle.stdout[offset..offset + length];
                    offset += length;
                    let expected = oracle.stdout[offset..offset + count * 2]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|x| i16::from_le_bytes(*x))
                        .collect::<Vec<_>>();
                    offset += count * 2;
                    let mut actual = vec![0; count];
                    decoder.decode(Some(packet), &mut actual, false).unwrap();
                    let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                    assert!(
                        first.is_none(),
                        "channels={channels:?} rate={rate} step={step} toc={} divergence={first:?}",
                        packet[0] >> 3
                    );
                }
                assert_eq!(offset, oracle.stdout.len());
            }
        }
    }
}

#[cfg(feature = "silk")]
#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn public_hybrid_loss_matches_pinned_api() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = root.parent().unwrap().parent().unwrap();
    for bandwidth in [1105] {
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for coded in [1, 2] {
                for channels in [Channels::Mono, Channels::Stereo] {
                    for size in [480, 960] {
                        for burst in [1, 7, 45] {
                            let oracle = Command::new("python3")
                                .arg("-B")
                                .arg(root.join("src/silk/celt_packet_loss_oracle.py"))
                                .arg(repository)
                                .arg(coded.to_string())
                                .arg(size.to_string())
                                .arg(burst.to_string())
                                .arg((channels as usize).to_string())
                                .arg(rate.to_string())
                                .arg("hybrid")
                                .arg(bandwidth.to_string())
                                .output()
                                .unwrap();
                            assert!(
                                oracle.status.success(),
                                "{}",
                                String::from_utf8_lossy(&oracle.stderr)
                            );
                            let mut decoder =
                                Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                            let mut offset = 0;
                            for step in 0..2 * (8 + burst) {
                                if step == 8 + burst {
                                    decoder.reset_state().unwrap();
                                }
                                let length = word(&oracle.stdout, &mut offset);
                                let count = word(&oracle.stdout, &mut offset);
                                let packet = &oracle.stdout[offset..offset + length];
                                offset += length;
                                let expected = oracle.stdout[offset..offset + count * 2]
                                    .as_chunks::<2>()
                                    .0
                                    .iter()
                                    .map(|x| i16::from_le_bytes(*x))
                                    .collect::<Vec<_>>();
                                offset += count * 2;
                                let mut actual = vec![0; count];
                                let produced = if length == 0 {
                                    if step % 2 == 0 {
                                        decoder
                                            .decode_loss(&mut actual, size * rate as usize / 48000)
                                    } else {
                                        decoder.decode(None, &mut actual, false)
                                    }
                                } else {
                                    decoder.decode(Some(packet), &mut actual, false)
                                }
                                .unwrap();
                                assert_eq!(produced, size * rate as usize / 48000);
                                let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                                assert!(
                                    first.is_none(),
                                    "coded={coded} channels={channels:?} size={size} burst={burst} step={step} divergence={first:?}"
                                );
                            }
                            assert_eq!(offset, oracle.stdout.len());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(all(feature = "silk", feature = "celt"))]
#[test]
#[ignore = "explicit public C oracle"]
fn public_hybrid_short_loss_matches_pinned_api() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = root.parent().unwrap().parent().unwrap();
    for bandwidth in [1105] {
        for rate in [8000, 12000, 16000, 24000, 48000] {
            for coded in [1, 2] {
                for channels in [Channels::Mono, Channels::Stereo] {
                    for size in [960] {
                        for burst in [1, 7, 45] {
                            let oracle = Command::new("python3")
                                .arg("-B")
                                .arg(root.join("src/silk/celt_packet_loss_oracle.py"))
                                .arg(repository)
                                .arg(coded.to_string())
                                .arg(size.to_string())
                                .arg(burst.to_string())
                                .arg((channels as usize).to_string())
                                .arg(rate.to_string())
                                .arg("hybrid")
                                .arg(bandwidth.to_string())
                                .arg("short")
                                .output()
                                .unwrap();
                            assert!(
                                oracle.status.success(),
                                "{}",
                                String::from_utf8_lossy(&oracle.stderr)
                            );
                            let mut decoder =
                                Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                            let mut offset = 0;
                            for step in 0..2 * (8 + burst) {
                                if step == 8 + burst {
                                    decoder.reset_state().unwrap();
                                }
                                let length = word(&oracle.stdout, &mut offset);
                                let count = word(&oracle.stdout, &mut offset);
                                let packet = &oracle.stdout[offset..offset + length];
                                offset += length;
                                let expected = oracle.stdout[offset..offset + count * 2]
                                    .as_chunks::<2>()
                                    .0
                                    .iter()
                                    .map(|x| i16::from_le_bytes(*x))
                                    .collect::<Vec<_>>();
                                offset += count * 2;
                                let mut actual = vec![0; count];
                                let produced = if length == 0 {
                                    decoder.decode_loss(&mut actual, rate as usize / 200)
                                } else {
                                    decoder.decode(Some(packet), &mut actual, false)
                                }
                                .unwrap();
                                assert_eq!(
                                    produced,
                                    if length == 0 {
                                        rate as usize / 200
                                    } else {
                                        size * rate as usize / 48000
                                    }
                                );
                                let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                                assert!(
                                    first.is_none(),
                                    "coded={coded} channels={channels:?} size={size} burst={burst} step={step} divergence={first:?}"
                                );
                            }
                            assert_eq!(offset, oracle.stdout.len());
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn missing_lbrr_uses_real_plc_and_preserves_current_packet() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for coded in [1, 2] {
            for channels in [Channels::Mono, Channels::Stereo] {
                for missing in [10, 20, 40, 60] {
                    let oracle = Command::new("python3")
                        .arg("-B")
                        .arg(root.join("src/silk/missing_lbrr_oracle.py"))
                        .args([
                            coded.to_string(),
                            (channels as usize).to_string(),
                            rate.to_string(),
                            missing.to_string(),
                        ])
                        .output()
                        .unwrap();
                    assert!(
                        oracle.status.success(),
                        "{}",
                        String::from_utf8_lossy(&oracle.stderr)
                    );
                    let mut decoder =
                        Decoder::new(SampleRate::from_hz(rate).unwrap(), channels).unwrap();
                    let mut offset = 0;
                    for record in 0..20 {
                        if record == 10 {
                            decoder.reset_state().unwrap();
                        }
                        let fec = word(&oracle.stdout, &mut offset) != 0;
                        let length = word(&oracle.stdout, &mut offset);
                        let count = word(&oracle.stdout, &mut offset);
                        let packet = &oracle.stdout[offset..offset + length];
                        offset += length;
                        assert!(!has_lbrr(packet, 20));
                        let expected = oracle.stdout[offset..offset + count * 2]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|x| i16::from_le_bytes(*x))
                            .collect::<Vec<_>>();
                        offset += count * 2;
                        let mut actual = vec![0; count];
                        decoder.decode(Some(packet), &mut actual, fec).unwrap();
                        let first = actual.iter().zip(&expected).position(|(a, b)| a != b);
                        assert!(
                            first.is_none(),
                            "rate={rate} coded={coded} channels={channels:?} missing={missing} record={record} divergence={first:?} actual={:?} expected={:?}",
                            &actual[..actual.len().min(40)],
                            &expected[..expected.len().min(40)]
                        );
                    }
                    assert_eq!(offset, oracle.stdout.len());
                }
            }
        }
    }
}

#[test]
fn missing_lbrr_fallback_has_real_state_and_invalid_duration_is_atomic() {
    let packet = include_bytes!("../src/silk/reference/nb_sine_mono.packet");
    assert!(!has_lbrr(packet, 20));
    let mut decoder = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
    let mut control = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
    decoder.decode(Some(packet), &mut [0; 160], false).unwrap();
    control.decode(Some(packet), &mut [0; 160], false).unwrap();
    let mut invalid = [123; 81];
    assert!(decoder.decode(Some(packet), &mut invalid, true).is_err());
    assert_eq!(invalid, [123; 81]);
    let mut actual = [0; 160];
    let mut expected = [0; 160];
    assert_eq!(
        decoder.decode(Some(packet), &mut actual, true).unwrap(),
        160
    );
    control.decode_loss(&mut expected, 160).unwrap();
    assert_eq!(actual, expected);
    assert!(actual.iter().any(|&sample| sample != 0));
    decoder.decode(Some(packet), &mut actual, false).unwrap();
    control.decode(Some(packet), &mut expected, false).unwrap();
    assert_eq!(actual, expected);
}
