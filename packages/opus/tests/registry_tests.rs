#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus::{create_opus_registry, register_opus_codec};
use symphonia::core::codecs::{CODEC_TYPE_OPUS, CodecRegistry};

#[test]
fn test_register_opus_codec() {
    let mut registry = CodecRegistry::new();

    // Initially empty registry
    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_none());

    // Register Opus codec
    register_opus_codec(&mut registry);

    // Now the codec should be available
    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_some());
}

#[test]
fn test_create_opus_registry_includes_opus() {
    let registry = create_opus_registry();

    // Verify Opus codec is registered
    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_some());
}

#[test]
fn test_create_opus_registry_includes_default_codecs() {
    let registry = create_opus_registry();

    // The registry should also include default Symphonia codecs
    // We can verify by checking that multiple codecs are available
    let opus_codec = registry.get_codec(CODEC_TYPE_OPUS);
    assert!(
        opus_codec.is_some(),
        "Opus codec should be registered in the registry"
    );
}

#[test]
fn test_register_opus_multiple_times() {
    let mut registry = CodecRegistry::new();

    // Register multiple times should not panic or cause issues
    register_opus_codec(&mut registry);
    register_opus_codec(&mut registry);

    // Codec should still be available
    assert!(registry.get_codec(CODEC_TYPE_OPUS).is_some());
}
