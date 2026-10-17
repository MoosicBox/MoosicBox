use super::*;

#[test]
fn silk_reset_restores_gain_scaling_and_entropy_voicing_banks() {
    let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();
    decoder.prev_gain_q16 = [123_456, 987_654];
    decoder.previous_frame_voiced = [true; 2];
    decoder.previous_pitch_lag = [Some(70), Some(90)];
    decoder.reset_decoder_state();
    assert_eq!(decoder.prev_gain_q16, [65536; 2]);
    assert_eq!(decoder.previous_frame_voiced, [false; 2]);
    assert_eq!(decoder.previous_pitch_lag, [None; 2]);
}

#[test]
fn duration_reconfiguration_preserves_prediction_and_rejects_invalid_input() {
    let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Stereo, 20).unwrap();
    decoder.previous_gain_indices = [Some(17), Some(23)];
    decoder.previous_pitch_lag = [Some(91), Some(123)];
    decoder.prev_gain_q16 = [123_456, 234_567];
    decoder.packet_frames = [2, 1];
    decoder.mono_history = 1234;
    decoder.ltp_state[0].lpc_history_q14[0] = 4321;
    decoder.stereo_state.as_mut().unwrap().s_mid = [12, 34];
    for ms in [10, 20, 10, 20] {
        decoder.set_frame_duration(ms).unwrap();
        assert_eq!(decoder.frame_size_ms, ms);
        assert_eq!(decoder.num_silk_frames, 1);
        assert_eq!(decoder.previous_gain_indices, [Some(17), Some(23)]);
        assert_eq!(decoder.previous_pitch_lag, [Some(91), Some(123)]);
        assert_eq!(decoder.prev_gain_q16, [123_456, 234_567]);
        assert_eq!(decoder.packet_frames, [2, 1]);
        assert_eq!(decoder.mono_history, 1234);
        assert_eq!(decoder.ltp_state[0].lpc_history_q14[0], 4321);
        assert_eq!(decoder.stereo_state.as_ref().unwrap().s_mid, [12, 34]);
    }
    for ms in [0, 5, 40, 60, 255] {
        assert!(decoder.set_frame_duration(ms).is_err());
        assert_eq!(decoder.frame_size_ms, 20);
        assert_eq!(decoder.previous_gain_indices, [Some(17), Some(23)]);
        assert_eq!(decoder.ltp_state[0].lpc_history_q14[0], 4321);
    }
}

#[test]
#[ignore = "private SILK component oracle; runner extension pending"]
fn comfort_noise_estimate_matches_pinned_cng_state() {
    let oracle = std::process::Command::new("python3")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/silk/cng_oracle.py"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let text = String::from_utf8(oracle.stdout).unwrap();
    let mut cng = super::super::cng::ComfortNoise::default();
    let mut lines = text.lines();
    for (frame, rate) in [8000, 8000, 12000, 12000, 16000, 16000]
        .into_iter()
        .enumerate()
    {
        let line = lines.next().unwrap();
        let order = if rate == 16000 { 16 } else { 10 };
        let subframes = if frame % 2 == 0 { 4 } else { 2 };
        let nlsf = (0..order)
            .map(|index| {
                i16::try_from((index + 1) * 32767 / (order + 1) + ((index % 3) - 1) * 33).unwrap()
            })
            .collect::<Vec<_>>();
        let gains = (0..subframes)
            .map(|index| 65536 * (index + 2 + i32::try_from(frame).unwrap()))
            .collect::<Vec<_>>();
        let excitation = (0..i32::try_from(rate / 200).unwrap() * subframes)
            .map(|index| (index * 31 - 1000) * 16)
            .collect::<Vec<_>>();
        cng.update(rate, frame % 3 != 1, &nlsf, &gains, &excitation);
        let expected = line
            .split_whitespace()
            .map(|value| value.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(cng.gain_q16, expected[0], "gain frame {frame}");
        assert_eq!(
            cng.nlsf_q15.map(i32::from),
            expected[1..17],
            "NLSF frame {frame}"
        );
        assert_eq!(
            cng.excitation_q14,
            expected[17..],
            "excitation frame {frame}"
        );
        let mut actual = vec![0; excitation.len()];
        cng.synthesize(
            &mut actual,
            match rate {
                8000 => Bandwidth::Narrowband,
                12000 => Bandwidth::Mediumband,
                _ => Bandwidth::Wideband,
            },
            200,
            65536,
        )
        .unwrap();
        let expected = lines
            .next()
            .unwrap()
            .strip_prefix("PCM ")
            .unwrap()
            .split_whitespace()
            .map(|x| x.parse::<i16>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "CNG PCM frame={frame}");
    }
}

#[test]
#[ignore = "private SILK component oracle; runner extension pending"]
fn recovery_glue_matches_pinned_c_loss_burst_and_recovery() {
    let oracle = std::process::Command::new("python3")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/silk/recovery_oracle.py"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let text = String::from_utf8(oracle.stdout).unwrap();
    let mut lines = text.lines();
    for n in [80, 160, 320] {
        let mut recovery = super::super::recovery::Recovery::default();
        for frame in 0..8 {
            let gain = if frame == 0 {
                0
            } else if frame < 3 {
                17
            } else if frame == 3 {
                101
            } else {
                37
            };
            let mut actual = (0..n)
                .map(|index| i16::try_from(((index * 13) % 71 - 35) * gain).unwrap())
                .collect::<Vec<_>>();
            recovery.process(&mut actual, frame < 3 || frame == 5);
            let expected = lines
                .next()
                .unwrap()
                .split_whitespace()
                .map(|value| value.parse::<i16>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "length={n} frame={frame}");
        }
    }
    assert!(lines.next().is_none());
}

#[test]
#[ignore = "private SILK component oracle; runner extension pending"]
fn concealment_parameter_update_matches_pinned_c() {
    let oracle = std::process::Command::new("python3")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/silk/plc_oracle.py"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let text = String::from_utf8(oracle.stdout).unwrap();
    let mut plc = super::super::plc::Concealment::default();
    let mut lines = text.lines();
    let mut saved_excitation = [0_i32; 320];
    for (frame, rate) in [8000, 8000, 12000, 12000, 16000, 16000]
        .into_iter()
        .enumerate()
    {
        let line = lines.next().unwrap();
        let order = if rate == 16000 { 16 } else { 10 };
        let n = if frame % 2 == 0 { 4 } else { 2 };
        let f = i32::try_from(frame).unwrap();
        let lags = (0..n)
            .map(|index| i16::try_from(rate / 1000 * (7 + index)).unwrap())
            .collect::<Vec<_>>();
        let taps = (0..n)
            .map(|index| {
                std::array::from_fn(|j| {
                    i8::try_from(if j == 2 {
                        60 + f * 12
                    } else {
                        3 + i32::try_from(index).unwrap()
                    })
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let lpc = (0..order)
            .map(|index| (if index % 2 == 1 { 1 } else { -1 }) * (index + 1) * 11)
            .collect::<Vec<_>>();
        let gains = (0..n)
            .map(|index| 65536 * (i32::try_from(index).unwrap() + 2 + f))
            .collect::<Vec<_>>();
        plc.update((rate, frame % 3 != 1), &lags, &taps, &lpc, &gains, 12288);
        let mut actual = vec![plc.pitch_q8];
        actual.extend(plc.taps_q14.map(i32::from));
        actual.extend(plc.lpc_q12.map(i32::from));
        actual.extend(plc.gains_q16);
        actual.extend([
            i32::from(plc.ltp_scale_q14),
            i32::try_from(plc.subframe).unwrap(),
            i32::try_from(plc.subframes).unwrap(),
        ]);
        let expected = line
            .split_whitespace()
            .map(|x| x.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "frame={frame}");
        let excitation = (0..rate / 200 * n)
            .map(|i| (i32::try_from(i).unwrap() * 71 - 1000) * 16)
            .collect::<Vec<_>>();
        saved_excitation[..excitation.len()].copy_from_slice(&excitation);
        let mut history = (0..rate / 50)
            .map(|i| i16::try_from((i * 13) % 1000).unwrap() - 500)
            .collect::<Vec<_>>();
        let mut lpc_history = (0..16).map(|i| (i - 8) * 1234).collect::<Vec<_>>();
        for loss in 0..3 {
            let actual = plc.synthesize(
                &saved_excitation,
                &history,
                &mut lpc_history,
                (rate / 200, n),
            );
            let expected = lines
                .next()
                .unwrap()
                .strip_prefix("PCM ")
                .unwrap()
                .split_whitespace()
                .map(|x| x.parse::<i16>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "PLC PCM frame={frame} loss={loss}");
            history.extend(actual);
            history.drain(..history.len() - rate / 50);
        }
    }
}

fn pcm(bytes: &[u8]) -> Vec<i16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|x| i16::from_le_bytes([x[0], x[1]]))
        .collect()
}

#[test]
fn reference_mb_stereo_channels_before_unmixing() {
    let packet = include_bytes!("reference/mb_mixed_stereo.packet");
    let mut range = RangeDecoder::new(&packet[3..]).unwrap();
    let mut decoder = SilkDecoder::new(SampleRate::Hz12000, Channels::Stereo, 20).unwrap();
    let flags = decoder.decode_header_bits(&mut range, true).unwrap();
    assert_eq!(
        decoder.decode_stereo_weights(&mut range).unwrap(),
        (-1033, 1033)
    );
    if !flags.side_vad_flags.as_ref().unwrap()[0] {
        assert!(!decoder.decode_mid_only_flag(&mut range).unwrap());
    }
    let mut mid = vec![0; 240];
    let mut side = vec![0; 240];
    decoder
        .decode_silk_frame_internal(&mut range, flags.mid_vad_flags[0], &mut mid, 0)
        .unwrap();
    decoder
        .decode_silk_frame_internal(
            &mut range,
            flags.side_vad_flags.as_ref().unwrap()[0],
            &mut side,
            1,
        )
        .unwrap();
    assert_eq!(
        mid,
        pcm(include_bytes!("reference/mb_mixed_stereo.mid.pcm")),
        "mid channel before stereo conversion"
    );
    assert_eq!(
        side,
        pcm(include_bytes!("reference/mb_mixed_stereo.side.pcm")),
        "side channel before stereo conversion"
    );
}

#[test]
fn reference_mb_stereo_raw_lr_before_api_delay() {
    let packet = include_bytes!("reference/mb_mixed_stereo.packet");
    // CBR code-3 framing: TOC, count/padding byte, zero padding length.
    assert_eq!(&packet[..3], &[0x2f, 0x41, 0]);
    let mut range = RangeDecoder::new(&packet[3..]).unwrap();
    let mut decoder = SilkDecoder::new(SampleRate::Hz12000, Channels::Stereo, 20).unwrap();
    let flags = decoder.decode_header_bits(&mut range, true).unwrap();
    let mut output = vec![0; 480];
    decoder
        .decode_silk_frame_stereo(
            &mut range,
            (
                flags.mid_vad_flags[0],
                flags.side_vad_flags.as_ref().unwrap()[0],
            ),
            &mut output,
        )
        .unwrap();
    assert_eq!(
        output,
        pcm(include_bytes!("reference/mb_mixed_stereo.lr.pcm"))
    );
}

#[test]
fn reference_nb_mono_decoded_channel_two_persistent_frames() {
    let packet = include_bytes!("reference/nb_sine_mono.packet");
    let expected = pcm(include_bytes!("reference/nb_sine_mono.decoded.pcm"));
    let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();
    for frame in 0..2 {
        decoder.begin_packet();
        let mut range = RangeDecoder::new(&packet[1..]).unwrap();
        let flags = decoder.decode_header_bits(&mut range, false).unwrap();
        let mut output = vec![0; 160];
        decoder
            .decode_silk_frame_internal(&mut range, flags.mid_vad_flags[0], &mut output, 0)
            .unwrap();
        let reference = &expected[frame * 160..(frame + 1) * 160];
        let first = output.iter().zip(reference).position(|(a, b)| a != b);
        assert!(first.is_none(), "frame {frame}, first divergence {first:?}");
    }
}

#[test]
fn reference_nb_mono_raw_api_two_persistent_frames() {
    let packet = include_bytes!("reference/nb_sine_mono.packet");
    let expected = pcm(include_bytes!("reference/nb_sine_mono.api.pcm"));
    let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();
    let mut resampler = crate::silk::resampler::SilkResampler::new(8000, 8000).unwrap();
    for frame in 0..2 {
        decoder.begin_packet();
        let mut range = RangeDecoder::new(&packet[1..]).unwrap();
        let flags = decoder.decode_header_bits(&mut range, false).unwrap();
        let mut output = vec![0; 160];
        decoder
            .decode_silk_frame(&mut range, flags.mid_vad_flags[0], &mut output)
            .unwrap();
        let actual = resampler.process(&output).unwrap();
        assert_eq!(
            actual,
            &expected[frame * 160..(frame + 1) * 160],
            "raw API frame {frame}"
        );
    }
}

#[test]
fn reference_wb_persistent_decoded_channel() {
    let packets: [&[u8]; 4] = [
        include_bytes!("reference/wb_sequence_0.packet"),
        include_bytes!("reference/wb_sequence_1.packet"),
        include_bytes!("reference/wb_sequence_2.packet"),
        include_bytes!("reference/wb_sequence_3.packet"),
    ];
    let expected = pcm(include_bytes!("reference/wb_sequence.decoded.pcm"));
    let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
    for (i, packet) in packets.iter().enumerate() {
        assert_eq!(packet[0] & 3, 3);
        let mut range =
            RangeDecoder::new(&packet[if packet[1] & 64 == 0 { 2 } else { 3 }..]).unwrap();
        decoder.begin_packet();
        let flags = decoder.decode_header_bits(&mut range, false).unwrap();
        let mut out = vec![0; 320];
        decoder
            .decode_silk_frame_internal(&mut range, flags.mid_vad_flags[0], &mut out, 0)
            .unwrap();
        let reference = &expected[i * 320..(i + 1) * 320];
        let first = out.iter().zip(reference).position(|(a, b)| a != b);
        assert!(
            first.is_none(),
            "packet {i}: {first:?}, actual {:?}, ref {:?}",
            first.map(|n| out[n]),
            first.map(|n| reference[n])
        );
    }
}

#[test]
fn reference_wb_persistent_full_raw_api_48khz() {
    let packets: [&[u8]; 4] = [
        include_bytes!("reference/wb_sequence_0.packet"),
        include_bytes!("reference/wb_sequence_1.packet"),
        include_bytes!("reference/wb_sequence_2.packet"),
        include_bytes!("reference/wb_sequence_3.packet"),
    ];
    let expected = pcm(include_bytes!("reference/wb_sequence.api48.pcm"));
    let mut decoder = SilkDecoder::new(SampleRate::Hz16000, Channels::Mono, 20).unwrap();
    let mut resampler = crate::silk::resampler::SilkResampler::new(16000, 48000).unwrap();
    for (i, packet) in packets.iter().enumerate() {
        let mut range =
            RangeDecoder::new(&packet[if packet[1] & 64 == 0 { 2 } else { 3 }..]).unwrap();
        decoder.begin_packet();
        let flags = decoder.decode_header_bits(&mut range, false).unwrap();
        let mut out = vec![0; 320];
        assert_eq!(
            decoder
                .decode_silk_frame(&mut range, flags.mid_vad_flags[0], &mut out)
                .unwrap(),
            320
        );
        let actual = resampler.process(&out).unwrap();
        assert_eq!(actual.len(), 960);
        assert_eq!(
            actual,
            &expected[i * 960..(i + 1) * 960],
            "48k raw packet {i}"
        );
    }
}

#[test]
fn reference_nb_impulse_decoded_channel() {
    let packet = include_bytes!("reference/nb_impulse.packet");
    let mut range = RangeDecoder::new(&packet[3..]).unwrap();
    let mut decoder = SilkDecoder::new(SampleRate::Hz8000, Channels::Mono, 20).unwrap();
    let flags = decoder.decode_header_bits(&mut range, false).unwrap();
    let mut out = vec![0; 160];
    decoder
        .decode_silk_frame_internal(&mut range, flags.mid_vad_flags[0], &mut out, 0)
        .unwrap();
    let reference = pcm(include_bytes!("reference/nb_impulse.decoded.pcm"));
    let first = out.iter().zip(&reference).position(|(a, b)| a != b);
    assert!(first.is_none(), "NB impulse first {first:?}");
}

#[test]
fn reference_wb_sequence_public_decoder_raw_48khz() {
    let packets: [&[u8]; 4] = [
        include_bytes!("reference/wb_sequence_0.packet"),
        include_bytes!("reference/wb_sequence_1.packet"),
        include_bytes!("reference/wb_sequence_2.packet"),
        include_bytes!("reference/wb_sequence_3.packet"),
    ];
    let expected = pcm(include_bytes!("reference/wb_sequence.api48.pcm"));
    let mut decoder = crate::Decoder::new(SampleRate::Hz48000, Channels::Mono).unwrap();
    for (i, packet) in packets.iter().enumerate() {
        let mut output = vec![0; 5760];
        let count = decoder.decode(Some(packet), &mut output, false).unwrap();
        assert_eq!(count, 960);
        assert_eq!(
            &output[..count],
            &expected[i * 960..(i + 1) * 960],
            "public API packet {i}"
        );
    }
}

#[test]
fn reference_stereo_coded_api_mono_and_stereo_persistent_and_reset() {
    for (packet, rate, mono, stereo) in [
        (
            include_bytes!("reference/mb_mixed.packet").as_slice(),
            SampleRate::Hz12000,
            include_bytes!("reference/mb_mixed.api1.pcm").as_slice(),
            include_bytes!("reference/mb_mixed.api2.pcm").as_slice(),
        ),
        (
            include_bytes!("reference/wb_mixed.packet").as_slice(),
            SampleRate::Hz16000,
            include_bytes!("reference/wb_mixed.api1.pcm").as_slice(),
            include_bytes!("reference/wb_mixed.api2.pcm").as_slice(),
        ),
    ] {
        for channels in [1, 2] {
            let expected = pcm(if channels == 1 { mono } else { stereo });
            for _reset in 0..2 {
                let mut decoder = SilkDecoder::new(rate, Channels::Stereo, 20).unwrap();
                let mut resamplers: Vec<_> = (0..channels)
                    .map(|_| {
                        crate::silk::resampler::SilkResampler::new(rate as u32, rate as u32)
                            .unwrap()
                    })
                    .collect();
                let samples = rate as usize / 50;
                for frame in 0..2 {
                    let offset = if packet[0].trailing_zeros() >= 2 {
                        1
                    } else if packet[1] & 64 == 0 {
                        2
                    } else {
                        3
                    };
                    let mut range = RangeDecoder::new(&packet[offset..]).unwrap();
                    decoder.begin_packet();
                    let flags = decoder.decode_header_bits(&mut range, true).unwrap();
                    let vad = (
                        flags.mid_vad_flags[0],
                        flags.side_vad_flags.as_ref().unwrap()[0],
                    );
                    let mut output = vec![0; samples * channels];
                    let count = if channels == 1 {
                        decoder.decode_silk_frame_stereo_to_mono(&mut range, vad, &mut output)
                    } else {
                        decoder.decode_silk_frame_stereo(&mut range, vad, &mut output)
                    }
                    .unwrap();
                    assert_eq!(count, samples);
                    let planes: Vec<_> = resamplers
                        .iter_mut()
                        .enumerate()
                        .map(|(ch, state)| {
                            state
                                .process(
                                    &output
                                        .iter()
                                        .skip(ch)
                                        .step_by(channels)
                                        .copied()
                                        .collect::<Vec<_>>(),
                                )
                                .unwrap()
                        })
                        .collect();
                    let actual: Vec<_> = (0..samples)
                        .flat_map(|n| planes.iter().map(move |p| p[n]))
                        .collect();
                    let reference =
                        &expected[frame * samples * channels..(frame + 1) * samples * channels];
                    let first = actual.iter().zip(reference).position(|(a, b)| a != b);
                    assert!(
                        first.is_none(),
                        "{rate:?} API {channels} frame {frame}, first={first:?}"
                    );
                }
            }
        }
    }
}

// Keep fixture inclusion explicit while keeping the matrix test reviewable.
macro_rules! persistent_fixture {
    ($rate:literal, $channels:literal, $name:literal) => {
        (
            $rate,
            $channels,
            include_bytes!(concat!("reference/a10/", $name, ".packets")).as_slice(),
            include_bytes!(concat!("reference/a10/", $name, ".api.pcm")).as_slice(),
        )
    };
}

#[test]
fn persistent_per_channel_nlsf_and_mid_only_restart_match_c() {
    for (rate, channels, packets, reference) in [
        persistent_fixture!(8000, 1, "8000-1-10"),
        persistent_fixture!(8000, 1, "8000-1-20"),
        persistent_fixture!(8000, 1, "8000-1-40"),
        persistent_fixture!(8000, 1, "8000-1-60"),
        persistent_fixture!(8000, 2, "8000-2-10"),
        persistent_fixture!(8000, 2, "8000-2-20"),
        persistent_fixture!(8000, 2, "8000-2-40"),
        persistent_fixture!(8000, 2, "8000-2-60"),
        persistent_fixture!(12000, 1, "12000-1-10"),
        persistent_fixture!(12000, 1, "12000-1-20"),
        persistent_fixture!(12000, 1, "12000-1-40"),
        persistent_fixture!(12000, 1, "12000-1-60"),
        persistent_fixture!(12000, 2, "12000-2-10"),
        persistent_fixture!(12000, 2, "12000-2-20"),
        persistent_fixture!(12000, 2, "12000-2-40"),
        persistent_fixture!(12000, 2, "12000-2-60"),
        persistent_fixture!(16000, 1, "16000-1-10"),
        persistent_fixture!(16000, 1, "16000-1-20"),
        persistent_fixture!(16000, 1, "16000-1-40"),
        persistent_fixture!(16000, 1, "16000-1-60"),
        persistent_fixture!(16000, 2, "16000-2-10"),
        persistent_fixture!(16000, 2, "16000-2-20"),
        persistent_fixture!(16000, 2, "16000-2-40"),
        persistent_fixture!(16000, 2, "16000-2-60"),
        persistent_fixture!(16000, 2, "transition"),
    ] {
        check_persistent_reference(rate, channels, packets, reference);
    }
    let trace = include_str!("reference/a10/transition.trace.jsonl");
    assert!(
        trace.contains("\"conditional\":1"),
        "C must exercise independently-coded side without LTP scaling"
    );
    assert!(trace.contains("\"stage\":\"mid_only\",\"value\":1"));
}

fn check_persistent_reference(rate: u32, channels: usize, packets: &[u8], reference: &[u8]) {
    for _reset in 0..2 {
        let mut decoder = crate::Decoder::new(
            SampleRate::from_hz(rate).unwrap(),
            if channels == 1 {
                Channels::Mono
            } else {
                Channels::Stereo
            },
        )
        .unwrap();
        let mut packets = packets;
        let mut expected = reference;
        let mut count = 0;
        while !packets.is_empty() {
            let n = u32::from_le_bytes(packets[..4].try_into().unwrap()) as usize;
            let packet = &packets[4..4 + n];
            packets = &packets[4 + n..];
            let samples = u32::from_le_bytes(expected[..4].try_into().unwrap()) as usize;
            let bytes = &expected[4..4 + samples * channels * 2];
            expected = &expected[4 + samples * channels * 2..];
            let mut actual = vec![0_i16; 5760 * 2];
            let got = decoder.decode(Some(packet), &mut actual, false).unwrap();
            assert_eq!(got, samples);
            assert_eq!(
                &actual[..got * channels],
                pcm(bytes),
                "{rate} packet {count}"
            );
            count += 1;
        }
        assert_eq!(count, 8);
        assert_eq!(expected, []);
    }
}

#[test]
fn nb_packet_five_nlsf_stabilization_matches_independent_c() {
    let before = vec![
        1831, 1809, 4573, 7144, 10830, 14662, 17592, 22016, 25600, 28544,
    ];
    let expected = vec![
        1819, 1822, 4573, 7144, 10830, 14662, 17592, 22016, 25600, 28544,
    ];
    assert_ne!(before, expected);
    let actual = SilkDecoder::stabilize_lsf(before, Bandwidth::Narrowband).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        SilkDecoder::limit_lpc_coefficients(&actual, Bandwidth::Narrowband).unwrap(),
        vec![8190, -4651, 447, -214, 661, -1369, 1524, -402, -351, 79]
    );
}
