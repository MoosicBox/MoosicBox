# MoosicBox Opus Codec

RFC 6716 compliant Opus audio codec decoder for Symphonia.

## Overview

The MoosicBox Opus package provides an Opus audio codec decoder that integrates with the Symphonia multimedia framework. It implements RFC 6716 packet parsing in Rust and uses libopus (via the `audiopus` crate) for audio decoding.

## Features

- ✅ Complete RFC 6716 packet structure parsing (all code types 0-3)
- ✅ TOC (Table of Contents) byte interpretation
- ✅ Frame length decoding with VBR and CBR support
- ✅ Padding extraction and handling
- ✅ DTX (Discontinuous Transmission) frame detection
- ✅ Symphonia `Decoder` trait implementation
- ✅ Support for all Opus modes (SILK, CELT, Hybrid) via libopus
- ✅ Codec registry integration

## Usage

Register the Opus decoder with a Symphonia codec registry:

```rust
use symphonia::core::codecs::CodecRegistry;
use moosicbox_opus::register_opus_codec;

let mut registry = CodecRegistry::new();
register_opus_codec(&mut registry);
```

Or create a registry with default Symphonia codecs plus Opus:

```rust
use moosicbox_opus::create_opus_registry;

let registry = create_opus_registry();
```

## Implementation

This package implements:

- **Packet parsing** (`packet.rs`) - Pure Rust parsing of Opus packet structures
- **TOC handling** (`toc.rs`) - Configuration and mode extraction
- **Frame processing** (`frame.rs`) - Frame length decoding and packing modes
- **Decoder integration** (`decoder.rs`) - Symphonia codec interface with libopus backend
- **Registry** (`registry.rs`) - Codec registration helpers

Audio decoding is performed by libopus through the `audiopus` crate.

## License

Licensed under the same terms as the MoosicBox project.

## See Also

- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio decoding framework
- [RFC 6716](https://tools.ietf.org/html/rfc6716) - Opus codec specification
- [Symphonia](https://github.com/pdeljanov/Symphonia) - Multimedia decoding framework
