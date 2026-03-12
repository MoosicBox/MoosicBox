# moosicbox_opus_native_libopus

**⚠️ TEMPORARY PACKAGE - NOT FOR PRODUCTION USE**

This package is an internal FFI wrapper around the official libopus reference implementation from Xiph.Org Foundation. It exists solely to generate test vectors for validating the pure Rust Opus decoder implementation in the parent `moosicbox_opus_native` package.

## Purpose

This package serves a temporary role in the development process:

- **Test Vector Generation**: Encodes and decodes audio samples using the official libopus implementation to create reference outputs

## Current Implementation

The package provides:

- **CMake Build Integration**: Compiles the official libopus library from source using the `build.rs` script (with fixed-point mode enabled for bit-exact decoding)
- **Raw FFI Bindings**: Unsafe extern functions for `opus_encoder_create`, `opus_encode`, `opus_decoder_create`, `opus_decode`, and their corresponding destroy functions
- **Safe Wrapper Module**: The `safe` module provides `Encoder` and `Decoder` types with memory-safe Rust interfaces
- **Minimal API Surface**: Only exposes the functionality needed for test vector generation

## Future Migration Plan

**This package will be removed once the pure Rust Opus decoder is stable.**

The migration timeline:

1. **Current Phase**: Using libopus FFI for test vector generation during `moosicbox_opus_native` development
2. **Transition Phase**: Once the Rust decoder passes all validation tests and achieves bit-exact output
3. **Final Phase**: Replace this FFI wrapper with a pure Rust encoder implementation for test vector generation
4. **Removal**: Delete this package entirely when no longer needed

See `../../../spec/opus-native/plan.md` for the detailed implementation roadmap of the pure Rust decoder.

## Upstream Source

This package builds the official Opus codec from Xiph.Org Foundation:

- **Repository**: https://gitlab.xiph.org/xiph/opus
- **Specification**: RFC 6716 - https://datatracker.ietf.org/doc/html/rfc6716

The upstream source is included as a git submodule in the `opus/` directory.

## Build Requirements

- CMake (for building libopus)
- C compiler toolchain
- Standard math library (`libm` on Unix systems)

The build process is handled automatically by `build.rs` and produces a static library that is linked into the Rust crate.

## Installation

This crate is internal and is not published to crates.io (`publish = false`).

Add it as a workspace dependency:

```toml
[dependencies]
moosicbox_opus_native_libopus = { workspace = true }
```

## Usage

Primary entry points are the safe wrappers in `safe`:

- `safe::Encoder::new(sample_rate, channels, application)`
- `safe::Encoder::encode(pcm, frame_size, output)`
- `safe::Decoder::new(sample_rate, channels)`
- `safe::Decoder::decode(data, output, frame_size, decode_fec)`

Use `OPUS_APPLICATION_AUDIO` or `OPUS_APPLICATION_VOIP` for encoder mode selection.
All fallible operations return `Result<_, safe::OpusError>`.

```rust
use moosicbox_opus_native_libopus::{OPUS_APPLICATION_AUDIO, safe::{Decoder, Encoder}};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut encoder = Encoder::new(48_000, 1, OPUS_APPLICATION_AUDIO)?;
let mut decoder = Decoder::new(48_000, 1)?;

let input_pcm = vec![0i16; 960];
let mut packet = vec![0u8; 4000];
let packet_len = encoder.encode(&input_pcm, 960, &mut packet)?;

let mut output_pcm = vec![0i16; 960];
let _decoded_samples = decoder.decode(&packet[..packet_len], &mut output_pcm, 960, false)?;
# Ok(())
# }
```

## License

Licensed under MPL-2.0.

## Related Packages

- **`moosicbox_opus_native`**: Parent package containing the pure Rust Opus decoder implementation
- **`moosicbox_opus_native_test_vectors`**: Test vector data generated using this package
