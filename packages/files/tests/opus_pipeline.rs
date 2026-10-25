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
async fn dropping_consumer_does_not_poison_next_transcode() {
    let result = get_audio_bytes_with_opus_encoder(source(), options(), config(48000))
        .await
        .unwrap();
    drop(result);
    assert_eq!(collect(config(48000)).await, collect(config(48000)).await);
}
