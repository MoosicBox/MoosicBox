//! Codec registry helpers for Opus decoder integration.
//!
//! This module provides convenience functions to register [`OpusDecoder`]
//! with Symphonia codec registries.

use symphonia::core::codecs::CodecRegistry;

use crate::decoder::OpusDecoder;

/// Register the Opus codec with a Symphonia codec registry.
///
/// This function registers [`OpusDecoder`] with the provided registry,
/// enabling Opus decoding support in Symphonia-based applications.
///
/// # Examples
///
/// ```rust
/// use symphonia::core::codecs::CodecRegistry;
/// use moosicbox_opus::register_opus_codec;
///
/// let mut registry = CodecRegistry::new();
/// register_opus_codec(&mut registry);
/// ```
pub fn register_opus_codec(registry: &mut CodecRegistry) {
    registry.register_all::<OpusDecoder>();
}

/// Create a codec registry with Opus support.
///
/// Creates a new [`CodecRegistry`] with default Symphonia codecs and Opus decoder registered.
///
/// # Examples
///
/// ```rust
/// use moosicbox_opus::create_opus_registry;
///
/// let registry = create_opus_registry();
/// // Use the registry with Symphonia to decode Opus streams
/// ```
#[must_use]
pub fn create_opus_registry() -> CodecRegistry {
    let mut registry = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut registry);
    register_opus_codec(&mut registry);
    registry
}
