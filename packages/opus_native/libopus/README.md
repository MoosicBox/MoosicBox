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
- **Application Mode Constants**: `OPUS_APPLICATION_AUDIO` and `OPUS_APPLICATION_VOIP` constants for encoder configuration
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

## Related Packages

- **`moosicbox_opus_native`**: Parent package containing the pure Rust Opus decoder implementation
- **`moosicbox_opus_native_test_vectors`**: Test vector data generated using this package
