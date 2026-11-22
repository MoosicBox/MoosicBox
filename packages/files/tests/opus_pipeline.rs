#![cfg(all(feature = "files", feature = "encoder-opus-native"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
//! Explicit Opus transcodes must not share legacy cached source or encoded bytes.
use futures::StreamExt;
use moosicbox_audio_decoder::{DecodeOptions, OpusDecodeBackend};
use moosicbox_audio_output::encoder::opus::{OpusEncodeBackend, OpusEncodeConfig};
use moosicbox_files::files::track::get_audio_bytes_with_opus_encoder;
use moosicbox_music_api::models::TrackSource;
use moosicbox_music_models::{ApiSource, AudioFormat, TrackApiSource};

fn source() -> TrackSource {
    TrackSource::LocalFilePath {
        format: AudioFormat::Opus,
        path: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/silk.ogg").to_owned(),
        track_id: None,
        source: TrackApiSource::Api(ApiSource::library()),
    }
}
fn options() -> DecodeOptions {
    DecodeOptions {
        opus_backend: OpusDecodeBackend::Native,
        ..Default::default()
    }
}
fn config(rate: u32) -> OpusEncodeConfig {
    OpusEncodeConfig {
        backend: OpusEncodeBackend::Native,
        channels: 1,
        sample_rate: rate,
        frame_size: rate as usize / 50,
        ..Default::default()
    }
}
async fn collect(config: OpusEncodeConfig) -> Vec<u8> {
    let result = get_audio_bytes_with_opus_encoder(source(), options(), config)
        .await
        .unwrap();
    let mut stream = result.stream;
    let mut bytes = Vec::new();
    while let Some(item) = stream.next().await {
        bytes.extend_from_slice(&item.unwrap().unwrap());
    }
    bytes
}

// Do not replace the process-global panic hook: tests may run concurrently.
// Libtest captures hook output; the final assertion lists every failed cell.
fn public_case_failure(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic payload")
}

async fn intended_public_file_mode_acceptance(
    mode: moosicbox_audio_output::encoder::opus::OpusEncodeMode,
    rates: &[u32],
    bitrates: &[u32],
) {
    use std::io::Cursor;
    use symphonia::core::{audio::Audio, formats::probe::Hint, io::MediaSourceStream};

    let original = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/silk.ogg"
    ))
    .unwrap();
    let decode = |bytes: Vec<u8>, seek| {
        let format = moosicbox_audio_decoder::create_format_probe()
            .probe(
                &Hint::new(),
                MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default()),
                Default::default(),
                Default::default(),
            )
            .unwrap();
        let buffers: Vec<_> = moosicbox_audio_decoder::unsync::decode_with_options_result(
            format,
            None,
            seek,
            options(),
        )
        .unwrap()
        .iter()
        .map(Result::unwrap)
        .collect();
        for buffer in &buffers {
            assert_eq!(
                buffer.spec().channels().count(),
                1,
                "file mono layout changed"
            );
            assert!(
                buffer
                    .iter_planes()
                    .flatten()
                    .all(|sample| sample.is_finite())
            );
        }
        let frames = buffers.iter().map(Audio::frames).sum::<usize>();
        assert!(
            buffers
                .iter()
                .flat_map(|audio| audio.iter_planes())
                .any(|plane| plane.iter().any(|sample| sample.abs() > 0.001))
        );
        frames
    };
    use futures::FutureExt;
    let mut failures = Vec::new();
    let mut attempted = 0;
    let mut cells = ProductCells::new(rates.len() * bitrates.len());
    let source_frames = decode(original.clone(), None);
    for &sample_rate in rates {
        for &bitrate in bitrates {
            let config = OpusEncodeConfig {
                mode,
                bitrate,
                ..config(sample_rate)
            };
            if !cells.start(&format!("{config:?}")) {
                continue;
            }
            attempted += 1;
            let outcome = std::panic::AssertUnwindSafe(async {
                // collect() unwraps both layers of terminal stream errors: a closed
                // public codec guard is a failure, never an accepted test outcome.
                let bytes = collect(config).await;
                assert_ne!(bytes, original, "must actually transcode: {config:?}");
                assert_eq!(
                    decode(bytes.clone(), None),
                    source_frames,
                    "public file clock: {config:?}"
                );
                assert_eq!(
                    decode(bytes.clone(), Some(0.05)),
                    source_frames - 2400,
                    "public file seek clock: {config:?}"
                );
                assert_eq!(
                    bytes,
                    collect(config).await,
                    "fresh file encoder state: {config:?}"
                );
            })
            .catch_unwind()
            .await;
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
    cells.complete();
    assert!(
        failures.is_empty(),
        "{} / {attempted} public file cells failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[switchy_async::test]
async fn intended_public_silk_low_rate_files_transcode_and_decode() {
    use moosicbox_audio_output::encoder::opus::OpusEncodeMode;
    intended_public_file_mode_acceptance(
        OpusEncodeMode::Silk,
        &[8000, 12000, 16000, 24000, 48000],
        &[16_000, 24_000, 32_000, 64_000],
    )
    .await;
}

#[switchy_async::test]
async fn intended_public_hybrid_low_rate_files_transcode_and_decode() {
    use moosicbox_audio_output::encoder::opus::OpusEncodeMode;
    intended_public_file_mode_acceptance(
        OpusEncodeMode::Hybrid,
        &[24000, 48000],
        &[32_000, 64_000],
    )
    .await;
}

#[switchy_async::test]
async fn intended_public_auto_low_rate_files_transcode_and_decode() {
    use moosicbox_audio_output::encoder::opus::OpusEncodeMode;
    intended_public_file_mode_acceptance(OpusEncodeMode::Auto, &[48000], &[32_000, 64_000]).await;
}

#[switchy_async::test]
async fn explicit_configurations_never_reuse_source_or_other_configuration_cache() {
    let original = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/silk.ogg"
    ))
    .unwrap();
    let a = collect(config(48000)).await;
    let b = collect(config(24000)).await;
    let again = collect(config(48000)).await;
    assert_ne!(a, original);
    assert_ne!(b, original);
    assert_ne!(a, b);
    assert_eq!(a, again);
    // First Ogg page has one 19-byte identification packet at offset 28.
    assert_eq!(&a[28..36], b"OpusHead");
    assert_eq!(&b[28..36], b"OpusHead");
    assert_eq!(u32::from_le_bytes(a[40..44].try_into().unwrap()), 48000);
    assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 24000);
}
#[switchy_async::test]
async fn explicit_auto_transcode_keeps_clock_and_does_not_reuse_mode_cache() {
    use moosicbox_audio_output::encoder::opus::OpusEncodeMode;

    let default = config(48_000);
    let auto = OpusEncodeConfig {
        mode: OpusEncodeMode::Auto,
        bitrate: 192_000,
        ..default
    };
    let fixed = collect(default).await;
    let adaptive = collect(auto).await;
    assert_ne!(
        adaptive, fixed,
        "explicit Auto must not reuse Default cache"
    );
    assert_eq!(adaptive, collect(auto).await);
    assert_eq!(fixed, collect(default).await);
    let audible = |bytes: &[u8]| {
        let eos = bytes
            .windows(4)
            .rposition(|window| window == b"OggS")
            .unwrap();
        let granule = u64::from_le_bytes(bytes[eos + 6..eos + 14].try_into().unwrap());
        let pre_skip = u16::from_le_bytes(bytes[38..40].try_into().unwrap());
        granule - u64::from(pre_skip)
    };
    assert_eq!(audible(&adaptive), audible(&fixed));
    let result = get_audio_bytes_with_opus_encoder(source(), options(), auto)
        .await
        .unwrap();
    drop(result);
    assert_eq!(
        adaptive,
        collect(auto).await,
        "cancellation must not retain Auto state"
    );
    let invalid = OpusEncodeConfig {
        frame_size: 120,
        ..auto
    };
    let result = get_audio_bytes_with_opus_encoder(source(), options(), invalid)
        .await
        .unwrap();
    let items: Vec<_> = result.stream.collect().await;
    assert!(items.iter().any(|item| match item {
        Err(_) => true,
        Ok(item) => item.is_err(),
    }));
    assert_eq!(
        adaptive,
        collect(auto).await,
        "failed Auto must not poison valid transcode"
    );
}

#[switchy_async::test]
async fn invalid_configuration_is_a_terminal_stream_error_not_cached_success() {
    let _valid = collect(config(48000)).await;
    let mut invalid = config(48000);
    invalid.frame_size = 0;
    let result = get_audio_bytes_with_opus_encoder(source(), options(), invalid)
        .await
        .unwrap();
    let items: Vec<_> = result.stream.collect().await;
    assert!(items.iter().any(|item| match item {
        Err(_) => true,
        Ok(item) => item.is_err(),
    }));
    let last = items.last().expect("must expose a terminal error");
    assert!(match last {
        Err(_) => true,
        Ok(item) => item.is_err(),
    });
}
#[switchy_async::test]
async fn multistream_transcode_has_explicit_mapping_header() {
    use moosicbox_audio_output::encoder::opus::{MultistreamLayout, OpusMultistreamEncodeConfig};
    let result = moosicbox_files::files::track::get_audio_bytes_with_multistream_encoder(
        source(),
        options(),
        OpusMultistreamEncodeConfig {
            sample_rate: 48000,
            frame_size: 960,
            layout: MultistreamLayout::new(1, 0, vec![0]).unwrap(),
            bitrates: vec![64000],
            mapping_family: 1,
        },
    )
    .await
    .unwrap();
    let mut bytes = Vec::new();
    for item in result.stream.collect::<Vec<_>>().await {
        bytes.extend_from_slice(&item.unwrap().unwrap());
    }
    assert_eq!(&bytes[28..36], b"OpusHead");
    assert_eq!(bytes[46], 1, "explicit family 1 must not become family 0");
    assert_eq!(&bytes[47..50], &[1, 0, 0]);
}

#[switchy_async::test]
async fn cancelling_after_first_output_does_not_cache_partial_transcode() {
    use moosicbox_audio_output::encoder::opus::OpusEncodeMode;

    for mode in [OpusEncodeMode::Default, OpusEncodeMode::Auto] {
        let config = OpusEncodeConfig {
            mode,
            bitrate: 192_000,
            ..config(48_000)
        };
        let expected = collect(config).await;
        let mut result = get_audio_bytes_with_opus_encoder(source(), options(), config)
            .await
            .unwrap();
        let first = result
            .stream
            .next()
            .await
            .expect("transcode must produce an item")
            .unwrap()
            .unwrap();
        assert!(
            !first.is_empty(),
            "cancellation must happen after actual output"
        );
        // Drop the live consumer without draining it. The subsequent request
        // must create fresh codec/filter state and not reuse cached partial bytes.
        drop(result);
        let actual = collect(config).await;
        assert_eq!(actual, expected, "mode={mode:?}");
        let eos = actual
            .windows(4)
            .rposition(|window| window == b"OggS")
            .unwrap();
        assert_ne!(actual[eos + 5] & 4, 0, "retry must include completed EOS");
    }
}

#[switchy_async::test]
async fn dropping_consumer_does_not_poison_next_transcode() {
    let result = get_audio_bytes_with_opus_encoder(source(), options(), config(48000))
        .await
        .unwrap();
    drop(result);
    assert_eq!(collect(config(48000)).await, collect(config(48000)).await);
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
