# Opus Test Vectors

This package provides test vectors for validating the moosicbox_opus_native decoder against RFC 6716 compliance.

## Status

**✅ Complete:** Test vector infrastructure and automatic generation using libopus
**✅ Working:** Test vectors are generated automatically during build via `build.rs`

## Directory Structure

Test vectors are generated at build time in `$OUT_DIR/generated/`:

```
$OUT_DIR/generated/
├── silk/
│   ├── nb/    # Narrowband (8 kHz) - 8 test cases
│   ├── mb/    # Mediumband (12 kHz) - 4 test cases
│   ├── wb/    # Wideband (16 kHz) - 4 test cases
│   └── swb/   # Super-wideband (24 kHz) - 2 test cases
├── celt/
│   └── fb/    # Fullband (48 kHz) - 1 test case
└── integration/  # Hybrid mode - 1 test case
```

## Test Vector Format

Each test vector consists of three files in a subdirectory:

- `packet.bin` - Raw Opus packet bytes (NOT OggOpus container)
- `expected.pcm` - Expected PCM output (16-bit signed little-endian)
- `meta.json` - Metadata about the test case

### meta.json Format

```json
{
  "sample_rate": 48000,
  "channels": 2,
  "frame_size_ms": 20,
  "mode": "celt"
}
```

## Test Vector Generation

Test vectors are automatically generated during the build process via `build.rs` using the `moosicbox_opus_native_libopus` encoder and decoder. This ensures:

- **Raw Opus packets**: Generated directly from libopus encoder (no container format)
- **Reference PCM output**: Decoded using libopus decoder for bit-exact comparison
- **Reproducible**: Same test vectors generated on every build
- **Comprehensive coverage**: Various signal types (silence, sine waves, impulses, white noise, mixed signals) across multiple bandwidths and channel configurations

### Signal Types Generated

The `build.rs` script generates test vectors with the following signal types:
- **Impulse**: Sharp impulses for testing transient response
- **Sine waves**: Various frequencies appropriate for each bandwidth
- **White noise**: Random noise for testing statistical behavior
- **Silence**: Zero samples for boundary testing
- **Mixed signals**: Combination of sine and noise
- **Quiet sine**: Low-amplitude sine waves

## Running Tests

```bash
# Run all integration tests (requires SILK feature)
cargo test -p moosicbox_opus_native --features silk --test integration_tests

# The tests will:
# 1. Load test vectors from $OUT_DIR/generated
# 2. Decode packets with moosicbox_opus_native decoder
# 3. Compare output to expected PCM (from libopus)
# 4. Assert bit-exact match (SNR = ∞)
```

**Note**: Test vectors are generated automatically during the build, so they're always available when tests run.

## Package Components

- **`src/lib.rs`**: Test vector loader and SNR calculation utilities
  - `TestVector::load()` - Load a single test vector from a directory
  - `TestVector::load_all()` - Load all test vectors from a directory
  - `calculate_snr()` - Calculate signal-to-noise ratio between reference and decoded PCM
  - `test_vectors_dir()` - Get path to generated test vectors
  - `vectors_available()` - Check if test vectors have been generated

- **`build.rs`**: Automatic test vector generation at build time
  - Uses `moosicbox_opus_native_libopus` encoder to create raw Opus packets
  - Uses `moosicbox_opus_native_libopus` decoder to create reference PCM output
  - Generates SILK vectors for NB/MB/WB/SWB bandwidths
  - Generates CELT fullband vector
  - Generates integration (hybrid mode) vector

- **`../tests/integration_tests.rs`**: Integration tests using generated vectors
  - `test_decode_silk_vectors` - Tests SILK decoder with all generated vectors
  - `test_sine_stereo_bit_exact` - Focused test for stereo SILK decoding
  - `test_decode_silk_vectors_skip_delay` - Tests with algorithmic delay compensation

## Dependencies

From `Cargo.toml`:
- **`moosicbox_opus_native_libopus`** (workspace dependency): libopus FFI bindings for generating reference test vectors
- **`serde_json`** (workspace dependency): Parsing `meta.json` metadata files
