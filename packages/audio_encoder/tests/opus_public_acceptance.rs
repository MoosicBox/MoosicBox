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
    let mut cells = ProductCells::new(rates.len() * 4 * bitrates.len());
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
                    if !cells.start(&format!("{config:?}")) {
                        continue;
                    }
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
                        let mut native = moosicbox_opus_native::Decoder::new(
                            moosicbox_opus_native::SampleRate::Hz48000,
                            if channels == 1 {
                                moosicbox_opus_native::Channels::Mono
                            } else {
                                moosicbox_opus_native::Channels::Stereo
                            },
                        )
                        .unwrap();
                        let mut native_pcm = Vec::new();
                        let mut audible = 0_u64;
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
                                // Auto is a selection policy, not a promise of initial CELT.
                                OpusEncodeMode::Auto => {
                                    assert!(toc < 32, "illegal Auto TOC: {config:?}, toc={toc}")
                                }
                                _ => unreachable!(),
                            }
                            audible += u64::from(packet.audible_samples());
                            let mut decoded = vec![0_i16; 5760 * usize::from(channels)];
                            let count = native
                                .decode(Some(&packet.data), &mut decoded, false)
                                .unwrap_or_else(|error| {
                                    panic!("native decode {config:?}: {error}")
                                });
                            assert_eq!(count, packet.duration as usize, "{config:?}");
                            native_pcm.extend_from_slice(
                                &decoded[packet.trim_start as usize * usize::from(channels)
                                    ..(count - packet.trim_end as usize) * usize::from(channels)],
                            );
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
                        assert_eq!(
                            native_pcm.len() as u64,
                            expected * u64::from(channels),
                            "{config:?}"
                        );
                        assert!(native_pcm.iter().any(|sample| *sample != 0), "{config:?}");
                        #[cfg(feature = "opus")]
                        {
                            assert_eq!(
                                reference_pcm.len() as u64,
                                expected * u64::from(channels),
                                "{config:?}"
                            );
                            for (index, (&native, &reference)) in
                                native_pcm.iter().zip(&reference_pcm).enumerate()
                            {
                                assert!(
                                    (i32::from(native) - i32::from(reference)).abs() <= 2,
                                    "C parity exceeds 2 LSB: {config:?}, interleaved_sample={index}, native={native}, reference={reference}"
                                );
                            }
                            assert!(
                                reference_pcm.iter().any(|sample| *sample != 0),
                                "{config:?}"
                            );
                        }
                    }));
                    cells.finish(
                        outcome
                            .as_ref()
                            .err()
                            .map(|payload| public_case_failure(payload.as_ref())),
                    );
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
    cells.complete();
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

// Diagnostic selection never changes the default acceptance matrix. Use --nocapture.
struct ProductCells {
    selector: Option<usize>,
    expected: usize,
    seen: usize,
    started: usize,
    passed: usize,
    current: String,
}
impl ProductCells {
    fn new(expected: usize) -> Self {
        let selector = std::env::var("MOOSICBOX_OPUS_PRODUCT_CELL")
            .ok()
            .map(|value| {
                value
                    .parse::<usize>()
                    .expect("MOOSICBOX_OPUS_PRODUCT_CELL must be a zero-based cell index")
            });
        Self {
            selector,
            expected,
            seen: 0,
            started: 0,
            passed: 0,
            current: String::new(),
        }
    }
    fn start(&mut self, context: &str) -> bool {
        let index = self.seen;
        self.seen += 1;
        if self.selector.is_some_and(|selected| selected != index) {
            return false;
        }
        self.started += 1;
        self.current = format!("cell={index} {context}");
        eprintln!("PRODUCT_CELL_START {}", self.current);
        true
    }
    fn finish(&mut self, error: Option<&str>) {
        if let Some(error) = error {
            eprintln!("PRODUCT_CELL_FAIL {}: {error}", self.current);
        } else {
            self.passed += 1;
            eprintln!("PRODUCT_CELL_PASS {}", self.current);
        }
    }
    fn complete(&self) {
        eprintln!(
            "PRODUCT_COVERAGE kind={} expected={} seen={} attempted={} passed={} failed={} omitted={}",
            if self.selector.is_some() {
                "PARTIAL_DIAGNOSTIC"
            } else {
                "FULL"
            },
            self.expected,
            self.seen,
            self.started,
            self.passed,
            self.started - self.passed,
            self.expected - self.started
        );
        assert_eq!(
            self.seen, self.expected,
            "acceptance matrix enumeration changed"
        );
        assert!(self.started > 0, "cell selector matched zero cells");
        if self.selector.is_none() {
            assert_eq!(self.started, self.expected);
        }
    }
}
