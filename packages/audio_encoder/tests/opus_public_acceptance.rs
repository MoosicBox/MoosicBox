#![cfg(feature = "opus-native")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Intended public low-rate acceptance: closed codec gates are failures, not skips.
//! No private practical constructor or injected adaptive state is used here.
use moosicbox_audio_encoder::opus_stream::{
    OpusEncodeBackend, OpusEncodeConfig, OpusEncodeMode, OpusStreamEncoder,
};
use moosicbox_ogg_opus::OggOpusReader;
use std::io::Cursor;

// Do not replace the process-global panic hook: tests may run concurrently.
// Libtest captures hook output; the final assertion lists every failed cell.
fn public_case_failure(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload")
}

fn accept(mode: OpusEncodeMode, rates: &[u32], bitrates: &[u32]) {
    let mut failures = Vec::new();
    let mut attempted = 0;
    for &sample_rate in rates {
        for channels in [1_u8, 2] {
            for divisor in [100, 50] {
                for &bitrate in bitrates {
                    let config = OpusEncodeConfig {
                        backend: OpusEncodeBackend::Native,
                        mode,
                        sample_rate,
                        channels,
                        bitrate,
                        frame_size: sample_rate as usize / divisor,
                    };
                    attempted += 1;
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let frames = 2457;
                        let pcm: Vec<_> = (0..frames)
                            .flat_map(|frame| {
                                (0..channels).map(move |channel| {
                                    (std::f32::consts::TAU
                                        * (220.0 + f32::from(channel) * 110.0)
                                        * frame as f32
                                        / sample_rate as f32)
                                        .sin()
                                        * 0.1
                                })
                            })
                            .collect();
                        let mut encoder = OpusStreamEncoder::new(Vec::new(), config)
                            .unwrap_or_else(|error| {
                                panic!("public constructor {config:?}: {error}")
                            });
                        for chunk in pcm.chunks(13 * usize::from(channels)) {
                            encoder.write(chunk).unwrap_or_else(|error| {
                                panic!("public encode {config:?}: {error}")
                            });
                        }
                        let bytes = encoder
                            .finish()
                            .unwrap_or_else(|error| panic!("public finish {config:?}: {error}"))
                            .expect("first finish must own the sink");
                        assert!(encoder.finish().unwrap().is_none(), "{config:?}");
                        assert!(encoder.write(&[]).is_err(), "{config:?}");
                        let mut reader = OggOpusReader::new(Cursor::new(bytes)).unwrap();
                        let expected = frames as u64 * u64::from(48_000 / sample_rate);
                        assert_eq!(reader.total_samples().unwrap(), expected, "{config:?}");
                        #[cfg(feature = "opus")]
                        let mut reference = opus::Decoder::new(
                            48_000,
                            if channels == 1 {
                                opus::Channels::Mono
                            } else {
                                opus::Channels::Stereo
                            },
                        )
                        .unwrap();
                        #[cfg(feature = "opus")]
                        let mut reference_pcm = Vec::new();
                        let mut audible = 0_u64;
                        let mut first_packet = true;
                        while let Some(packet) = reader.next_packet().unwrap() {
                            let toc = packet.data[0] >> 3;
                            match mode {
                                OpusEncodeMode::Silk => {
                                    assert!(toc < 12, "SILK fallback: {config:?}, toc={toc}")
                                }
                                OpusEncodeMode::Hybrid => assert!(
                                    (12..16).contains(&toc),
                                    "Hybrid fallback: {config:?}, toc={toc}"
                                ),
                                OpusEncodeMode::Auto if first_packet => assert!(
                                    toc >= 16,
                                    "initial low-rate Auto policy: {config:?}, toc={toc}"
                                ),
                                OpusEncodeMode::Auto => {}
                                _ => unreachable!(),
                            }
                            first_packet = false;
                            audible += u64::from(packet.audible_samples());
                            #[cfg(feature = "opus")]
                            {
                                let mut pcm = vec![0_i16; 5760 * usize::from(channels)];
                                let count =
                                    reference.decode(&packet.data, &mut pcm, false).unwrap();
                                assert_eq!(count, packet.duration as usize, "{config:?}");
                                reference_pcm.extend_from_slice(
                                    &pcm[packet.trim_start as usize * usize::from(channels)
                                        ..(count - packet.trim_end as usize)
                                            * usize::from(channels)],
                                );
                            }
                        }
                        assert_eq!(audible, expected, "{config:?}");
                        #[cfg(feature = "opus")]
                        {
                            assert_eq!(
                                reference_pcm.len() as u64,
                                expected * u64::from(channels),
                                "{config:?}"
                            );
                            assert!(
                                reference_pcm.iter().any(|sample| *sample != 0),
                                "{config:?}"
                            );
                        }
                    }));
                    if let Err(payload) = outcome {
                        failures.push(format!(
                            "{config:?}: {}",
                            public_case_failure(payload.as_ref())
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {attempted} public encoder cells failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn public_silk_intended_low_rate_matrix() {
    accept(
        OpusEncodeMode::Silk,
        &[8000, 12000, 16000, 24000, 48000],
        &[16_000, 24_000, 32_000, 64_000],
    );
}

#[test]
fn public_hybrid_intended_low_rate_matrix() {
    accept(OpusEncodeMode::Hybrid, &[24_000, 48_000], &[32_000, 64_000]);
}

#[test]
fn public_auto_intended_low_rate_matrix() {
    accept(OpusEncodeMode::Auto, &[48_000], &[32_000, 64_000]);
}
