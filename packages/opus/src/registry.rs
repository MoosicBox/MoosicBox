use symphonia::core::codecs::CodecRegistry;

use crate::decoder::OpusDecoder;

/// Register Opus codec with the provided registry.
pub fn register_opus_codec(registry: &mut CodecRegistry) {
    registry.register_all::<OpusDecoder>();
}

/// Create a codec registry with Opus support.
#[must_use]
pub fn create_opus_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    register_opus_codec(&mut registry);
    registry
}
