#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::{create_opus_registry, register_opus_codec};
use symphonia::core::codecs::{CODEC_TYPE_OPUS, CodecRegistry};

#[test_log::test]
fn test_register_opus_codec() {
    let mut registry = CodecRegistry::new();
    register_opus_codec(&mut registry);

    // Verify the codec is registered by checking we can get a descriptor
    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_some());
}

#[test_log::test]
fn test_create_opus_registry() {
    let registry = create_opus_registry();

    // Verify Opus is registered
    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_some());

    // Verify default codecs are also registered (e.g., MP3)
    assert!(
        registry
            .get_codec(symphonia::core::codecs::CODEC_TYPE_MP3)
            .is_some()
    );
}

#[test_log::test]
fn test_register_opus_codec_multiple_times() {
    let mut registry = CodecRegistry::new();

    // Should be safe to register multiple times
    register_opus_codec(&mut registry);
    register_opus_codec(&mut registry);

    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_some());
}
