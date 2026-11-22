//! Container-independent multistream framing and native channel routing tests.
#[cfg(feature = "celt")]
use moosicbox_opus_native::{Channels, Decoder};
use moosicbox_opus_native::{
    SampleRate,
    framing::parse_frames,
    multistream::{MultistreamDecoder, MultistreamLayout, join_packets, split_packet},
};

#[test]
fn layout_validation_and_explicit_mapping() {
    assert!(MultistreamLayout::new(0, 0, vec![0]).is_err());
    assert!(MultistreamLayout::new(1, 2, vec![0]).is_err());
    assert!(MultistreamLayout::new(128, 128, vec![0]).is_err());
    assert!(MultistreamLayout::new(1, 0, vec![]).is_err());
    assert!(MultistreamLayout::new(1, 0, vec![1]).is_err());
    assert!(MultistreamLayout::new(1, 0, vec![0; 256]).is_err());
    let layout = MultistreamLayout::new(2, 1, vec![2, 0, 1, 255, 2]).unwrap();
    assert_eq!(layout.channels(), 5);
    assert_eq!(layout.streams(), 2);
    assert_eq!(layout.coupled_streams(), 1);
    assert_eq!(layout.mapping(), &[2, 0, 1, 255, 2]);
}

#[test]
fn all_packet_codes_round_trip_without_changing_frames() {
    let packets = [
        vec![0x80, 10, 11],
        vec![0x81, 10, 11, 12, 13],
        vec![0x82, 1, 10, 11, 12],
        vec![0x83, 0x83, 1, 2, 10, 11, 12, 13, 14, 15],
        vec![0x83, 0x43, 2, 10, 11, 12, 0, 0],
    ];
    for packet in packets {
        let original = parse_frames(&packet).unwrap();
        let joined = join_packets(&[packet.clone(), packet.clone(), packet.clone()]).unwrap();
        let split = split_packet(&joined, 3).unwrap();
        for elementary in split {
            assert_eq!(elementary[0] & 0xfc, packet[0] & 0xfc);
            assert_eq!(parse_frames(&elementary).unwrap(), original);
        }
    }
}

#[test]
fn independently_written_self_delimited_packets() {
    let split = split_packet(&[0x80, 1, 0xaa, 0x80, 0xbb], 2).unwrap();
    assert_eq!(parse_frames(&split[0]).unwrap(), vec![&[0xaa][..]]);
    assert_eq!(parse_frames(&split[1]).unwrap(), vec![&[0xbb][..]]);
    let split = split_packet(&[0x81, 1, 10, 11, 0x81, 12, 13], 2).unwrap();
    assert_eq!(parse_frames(&split[0]).unwrap(), vec![&[10][..], &[11][..]]);
    let split = split_packet(&[0x82, 1, 2, 10, 11, 12, 0x81, 13, 14], 2).unwrap();
    assert_eq!(
        parse_frames(&split[0]).unwrap(),
        vec![&[10][..], &[11, 12][..]]
    );
    // Chained padding: 255 contributes 254, final 1 contributes 1.
    let mut packet = vec![0x83, 0xc2, 255, 1, 1, 2, 10, 11, 12];
    packet.extend([0; 255]);
    packet.extend([0x81, 13, 14]);
    let split = split_packet(&packet, 2).unwrap();
    assert_eq!(
        parse_frames(&split[0]).unwrap(),
        vec![&[10][..], &[11, 12][..]]
    );
}

#[test]
fn long_frames_and_malformed_framing() {
    for size in [0, 1, 251, 252, 253, 254, 255, 256, 1275] {
        let mut packet = vec![0x80];
        packet.resize(size + 1, 123);
        let joined = join_packets(&[packet.clone(), packet.clone()]).unwrap();
        let split = split_packet(&joined, 2).unwrap();
        assert_eq!(
            parse_frames(&split[0]).unwrap(),
            parse_frames(&packet).unwrap()
        );
        for end in 0..joined.len() {
            let _ = split_packet(&joined[..end], 2);
        }
    }
    for packet in [
        vec![],
        vec![0x80],
        vec![0x80, 252],
        vec![0x83, 0],
        vec![0x83, 0x41, 255],
    ] {
        assert!(split_packet(&packet, 2).is_err());
    }
    assert!(split_packet(&[0x80], 0).is_err());
    assert!(join_packets(&[]).is_err());
    assert!(join_packets(&[vec![0x80, 1], vec![0x88, 2]]).is_err());
    assert!(join_packets(&[vec![0x80; 1277]]).is_err());
    assert!(split_packet(&[0x80, 1, 10, 0x88, 20], 2).is_err());
}

#[test]
fn empty_frames_and_deterministic_malformed_headers_do_not_panic() {
    // Empty compressed frames are legal (DTX); code one carries two of them.
    assert_eq!(parse_frames(&[0x81]).unwrap(), vec![&[][..], &[][..]]);
    let joined = join_packets(&[vec![0x81], vec![0x81]]).unwrap();
    let split = split_packet(&joined, 2).unwrap();
    assert!(
        split
            .iter()
            .all(|p| parse_frames(p).unwrap() == vec![&[][..], &[][..]])
    );
    for toc in 0..=u8::MAX {
        for header in 0..=u8::MAX {
            for packet in [vec![toc, header], vec![toc, header, 0, 0]] {
                let _ = split_packet(&packet, 2);
            }
        }
    }
}

#[test]
fn initial_loss_is_silence_and_reset_preserves_gain() {
    let layout = MultistreamLayout::new(2, 1, vec![2, 0, 1, 255]).unwrap();
    let mut decoder = MultistreamDecoder::new(SampleRate::Hz48000, layout).unwrap();
    decoder.set_gain(1234);
    let mut pcm = vec![77; 120 * 4 + 1];
    assert_eq!(decoder.decode_loss(&mut pcm, 120).unwrap(), 120);
    assert!(pcm[..480].iter().all(|&x| x == 0));
    assert_eq!(pcm[480], 77);
    decoder.reset_state().unwrap();
    assert_eq!(decoder.gain(), 1234);
    assert!(decoder.decode_loss(&mut pcm, 121).is_err());
    assert!(decoder.decode_loss(&mut pcm, 0).is_err());
    assert!(decoder.decode_loss(&mut pcm, 240).is_err());
}

#[cfg(feature = "celt")]
fn encoded_fixture() -> (MultistreamLayout, Vec<u8>) {
    use moosicbox_opus_native::{encoder::EncoderConfig, multistream::MultistreamEncoder};
    let layout = MultistreamLayout::new(2, 1, vec![2, 0, 1, 255]).unwrap();
    let configs = vec![
        EncoderConfig {
            sample_rate: SampleRate::Hz48000,
            channels: Channels::Stereo,
            bitrate: 128_000,
            frame_size: 960,
        },
        EncoderConfig {
            sample_rate: SampleRate::Hz48000,
            channels: Channels::Mono,
            bitrate: 64_000,
            frame_size: 960,
        },
    ];
    let mut encoder = MultistreamEncoder::new(layout.clone(), configs).unwrap();
    let pcm: Vec<_> = (0..960)
        .flat_map(|i| {
            let a = if i % 51 < 25 { 0.2 } else { -0.2 };
            let b = if i % 37 < 18 { 0.3 } else { -0.3 };
            let c = if i % 23 < 11 { 0.1 } else { -0.1 };
            [a, b, c, 0.8]
        })
        .collect();
    let packet = encoder.encode(&pcm).unwrap();
    assert!(encoder.encode(&pcm[..100]).is_err());
    let tail = encoder.finish().unwrap();
    assert_eq!(tail.len(), 1);
    assert_eq!(split_packet(&tail[0], 2).unwrap().len(), 2);
    assert!(encoder.finish().unwrap().is_empty());
    assert!(encoder.encode(&pcm).is_err());
    encoder.reset_state().unwrap();
    assert_eq!(encoder.encode(&pcm).unwrap(), packet);
    (layout, packet)
}

#[cfg(feature = "celt")]
fn check_native_routing(layout: MultistreamLayout, packet: &[u8]) {
    let elementary = split_packet(packet, 2).unwrap();
    let mut stereo_decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
    let mut mono_decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    let mut stereo = vec![0; 1920];
    let mut mono = vec![0; 960];
    stereo_decoder
        .decode(Some(&elementary[0]), &mut stereo, false)
        .unwrap();
    mono_decoder
        .decode(Some(&elementary[1]), &mut mono, false)
        .unwrap();
    assert!(mono.iter().any(|&x| x != 0));
    assert!(stereo.as_chunks::<2>().0.iter().any(|x| x[0] != x[1]));
    let mut decoder = MultistreamDecoder::new(SampleRate::Hz48000, layout).unwrap();
    let mut output = vec![1; 3840];
    assert_eq!(
        decoder
            .decode(Some(packet), &mut output, 960, false)
            .unwrap(),
        960
    );
    for i in 0..960 {
        assert_eq!(
            &output[i * 4..i * 4 + 4],
            &[mono[i], stereo[i * 2], stereo[i * 2 + 1], 0]
        );
    }
    let mut loss = vec![1; 3840];
    assert_eq!(decoder.decode_loss(&mut loss, 960).unwrap(), 960);
    assert!(loss.as_chunks::<4>().0.iter().all(|x| x[3] == 0));
    decoder.reset_state().unwrap();
    let mut reset_output = vec![0; 3840];
    decoder
        .decode(Some(packet), &mut reset_output, 960, false)
        .unwrap();
    assert_eq!(reset_output, output);

    // Gain must be applied by each elementary decoder, including PLC, not by
    // mapping postprocessing. Duplicate outputs remain bit-identical.
    let duplicate_layout = MultistreamLayout::new(2, 1, vec![2, 0, 1, 255, 2]).unwrap();
    let mut gained = MultistreamDecoder::new(SampleRate::Hz48000, duplicate_layout).unwrap();
    gained.set_gain(-1536);
    stereo_decoder.reset_state().unwrap();
    mono_decoder.reset_state().unwrap();
    stereo_decoder.set_gain(-1536);
    mono_decoder.set_gain(-1536);
    stereo_decoder
        .decode(Some(&elementary[0]), &mut stereo, false)
        .unwrap();
    mono_decoder
        .decode(Some(&elementary[1]), &mut mono, false)
        .unwrap();
    let mut gained_output = vec![123; 960 * 5];
    gained
        .decode(Some(packet), &mut gained_output, 960, false)
        .unwrap();
    for i in 0..960 {
        assert_eq!(
            &gained_output[i * 5..i * 5 + 5],
            &[mono[i], stereo[i * 2], stereo[i * 2 + 1], 0, mono[i]]
        );
    }
    gained.reset_state().unwrap();
    let mut gained_reset = vec![0; 960 * 5];
    gained
        .decode(Some(packet), &mut gained_reset, 960, false)
        .unwrap();
    assert_eq!(gained_reset, gained_output);
    gained.decode_loss(&mut gained_output, 960).unwrap();
    stereo_decoder.decode_loss(&mut stereo, 960).unwrap();
    mono_decoder.decode_loss(&mut mono, 960).unwrap();
    for i in 0..960 {
        assert_eq!(
            &gained_output[i * 5..i * 5 + 5],
            &[mono[i], stereo[i * 2], stereo[i * 2 + 1], 0, mono[i]]
        );
    }
}

#[cfg(feature = "celt")]
#[test]
fn encoding_routes_distinct_channels_and_reset_reproduces_packet() {
    let (layout, packet) = encoded_fixture();
    check_native_routing(layout, &packet);
}

#[cfg(feature = "celt")]
#[test]
fn lookahead_is_exact_at_every_rate_and_stable_through_lifecycle() {
    use moosicbox_opus_native::{encoder::EncoderConfig, multistream::MultistreamEncoder};
    for (rate, delay) in [
        (SampleRate::Hz8000, 32),
        (SampleRate::Hz12000, 42),
        (SampleRate::Hz16000, 52),
        (SampleRate::Hz24000, 72),
        (SampleRate::Hz48000, 120),
    ] {
        let layout = MultistreamLayout::new(2, 1, vec![0, 1, 2]).unwrap();
        let frame_size = rate as usize / 50;
        let configs = [Channels::Stereo, Channels::Mono]
            .into_iter()
            .map(|channels| EncoderConfig {
                sample_rate: rate,
                channels,
                bitrate: 128_000,
                frame_size,
            })
            .collect();
        let mut encoder = MultistreamEncoder::new(layout, configs).unwrap();
        assert_eq!(encoder.lookahead(), delay);
        assert!(encoder.finish().unwrap().is_empty());
        assert_eq!(encoder.lookahead(), delay);
        encoder.reset_state().unwrap();
        encoder.encode(&vec![0.0; frame_size * 3]).unwrap();
        assert_eq!(encoder.lookahead(), delay);
        assert_eq!(encoder.finish().unwrap().len(), 1);
        assert_eq!(encoder.lookahead(), delay);
        encoder.reset_state().unwrap();
        assert_eq!(encoder.lookahead(), delay);
    }
}

#[cfg(feature = "celt")]
#[test]
fn constructor_rejects_unequal_rates_or_frames_before_use() {
    use moosicbox_opus_native::{encoder::EncoderConfig, multistream::MultistreamEncoder};
    let layout = MultistreamLayout::new(2, 0, vec![0, 1]).unwrap();
    let first = EncoderConfig {
        channels: Channels::Mono,
        ..EncoderConfig::default()
    };
    for second in [
        EncoderConfig {
            sample_rate: SampleRate::Hz24000,
            frame_size: 480,
            ..first
        },
        EncoderConfig {
            frame_size: 480,
            ..first
        },
    ] {
        assert!(MultistreamEncoder::new(layout.clone(), vec![first, second]).is_err());
    }
}

#[cfg(feature = "celt")]
#[test]
fn encoder_rejects_unmapped_coded_channels() {
    use moosicbox_opus_native::{encoder::EncoderConfig, multistream::MultistreamEncoder};
    let layout = MultistreamLayout::new(1, 1, vec![0, 255]).unwrap();
    assert!(
        MultistreamEncoder::new(
            layout,
            vec![EncoderConfig {
                channels: Channels::Stereo,
                ..EncoderConfig::default()
            }]
        )
        .is_err()
    );
}

// Explicit opt-in: requires a qualified portable pinned oracle. Framing/routing
// and exact waveform parity are separate tests so neither can mask the other.
#[cfg(feature = "celt")]
#[test]
#[ignore = "requires MOOSICBOX_OPUS_REFERENCE_ORACLE, explicit pinned portable executable"]
fn pinned_c_multistream_interoperability() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/multistream_oracle.py");
    assert!(std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").is_some());
    let fixture = Command::new("python3")
        .args(["-B", script, "fixture"])
        .output()
        .unwrap();
    assert!(
        fixture.status.success(),
        "{}",
        String::from_utf8_lossy(&fixture.stderr)
    );
    let text = String::from_utf8(fixture.stdout).unwrap();
    let packet: Vec<u8> = text
        .lines()
        .next()
        .unwrap()
        .split(',')
        .map(|x| x.parse().unwrap())
        .collect();
    let layout = MultistreamLayout::new(2, 1, vec![2, 0, 1, 255]).unwrap();
    check_native_routing(layout, &packet);
    let (_, packet) = encoded_fixture();
    let mut child = Command::new("python3")
        .args(["-B", script, "decode"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let list = packet
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    write!(child.stdin.take().unwrap(), "{{\"packet\":[{list}]}}").unwrap();
    let decoded = child.wait_with_output().unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    let text = String::from_utf8(decoded.stdout).unwrap();
    assert_eq!(text.trim().split(',').count(), 3840);
}

/// No tolerance or self-roundtrip substitution: compare C-produced PCM directly.
#[cfg(feature = "celt")]
#[test]
#[ignore = "requires MOOSICBOX_OPUS_REFERENCE_ORACLE, exact pinned C waveform qualification"]
fn pinned_c_multistream_exact_pcm() {
    let fixture = std::process::Command::new("python3")
        .args([
            "-B",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/multistream_oracle.py"),
            "fixture",
        ])
        .output()
        .unwrap();
    assert!(
        fixture.status.success(),
        "{}",
        String::from_utf8_lossy(&fixture.stderr)
    );
    let text = String::from_utf8(fixture.stdout).unwrap();
    let mut lines = text.lines();
    let packet: Vec<u8> = lines
        .next()
        .unwrap()
        .split(',')
        .map(|x| x.parse().unwrap())
        .collect();
    let expected: Vec<Vec<i16>> = lines
        .map(|line| line.split(',').map(|x| x.parse().unwrap()).collect())
        .collect();
    assert_eq!(expected.len(), 4);
    let layout = MultistreamLayout::new(2, 1, vec![2, 0, 1, 255]).unwrap();
    let mut decoder = MultistreamDecoder::new(SampleRate::Hz48000, layout).unwrap();
    let mut actual = Vec::new();
    let mut pcm = vec![0; 3840];
    decoder.decode(Some(&packet), &mut pcm, 960, false).unwrap();
    actual.push(pcm.clone());
    decoder.decode_loss(&mut pcm, 960).unwrap();
    actual.push(pcm.clone());
    decoder.reset_state().unwrap();
    decoder.decode(Some(&packet), &mut pcm, 960, false).unwrap();
    actual.push(pcm.clone());
    decoder.set_gain(-1536);
    decoder.reset_state().unwrap();
    decoder.decode(Some(&packet), &mut pcm, 960, false).unwrap();
    actual.push(pcm);
    let mut mismatches = Vec::new();
    for (state, (native, reference)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(native.len(), reference.len());
        let differences: Vec<_> = native
            .iter()
            .zip(reference)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .collect();
        if let Some((index, (a, b))) = differences.first() {
            mismatches.push(format!("state={state} mismatches={} first_sample={index} channel={} native={a} reference={b}", differences.len(), index % 4));
        }
    }
    assert!(
        mismatches.is_empty(),
        "exact C waveform parity failed (decode/PLC/reset/gain): {}",
        mismatches.join("; ")
    );
}
