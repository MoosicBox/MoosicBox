# Opus Test Vectors

This directory contains test vectors for validating the moosicbox_opus_native decoder against RFC 6716 compliance.

## Status

**✅ Complete:** Test vector infrastructure (loader, SNR calculation, test harness)  
**🔴 Blocked:** Actual test vector generation requires raw Opus packets

## Directory Structure

```
test-vectors/
├── silk/
│   ├── nb/    # Narrowband (8 kHz)
│   ├── mb/    # Mediumband (12 kHz)
│   ├── wb/    # Wideband (16 kHz)
│   └── swb/   # Super-wideband (24 kHz)
├── celt/
│   ├── nb/    # Narrowband (8 kHz)
│   ├── wb/    # Wideband (16 kHz)
│   ├── swb/   # Super-wideband (24 kHz)
│   └── fb/    # Fullband (48 kHz)
├── integration/  # Hybrid mode, transitions
└── edge-cases/   # Malformed packets, boundaries
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

## Current Issue: Raw Packet Generation

### Problem

The `opusenc`/`opusdec` tools (from `opus-tools` package) work with **OggOpus container format**, not raw Opus packets. Our decoder expects raw packets as specified in RFC 6716.

**What we have:**
- `opusenc` creates `.opus` files (Ogg container with headers/metadata)
- `opusdec` decodes `.opus` files to PCM

**What we need:**
- Raw Opus packet bytes (just the codec bitstream, no container)
- Corresponding reference PCM output from libopus

### Attempted Solutions

1. **✅ Test Infrastructure:** Complete - loader, SNR, test harness all working
2. **❌ OggOpus extraction:** `.opus` files include Ogg headers, not just packets
3. **❌ Synthetic packets:** Hand-crafted packets fail due to complex internal state requirements
4. **🔄 In Progress:** Investigating libopus `opus_demo` tool

### Recommended Solution: opus_demo

The `opus_demo` tool from libopus source provides exactly what we need:
- Encodes raw PCM → raw Opus packets (no container)
- Decodes raw Opus packets → raw PCM
- Perfect for generating test vectors

**How to build opus_demo:**

```bash
# Clone libopus
git clone https://gitlab.xiph.org/xiph/opus.git
cd opus

# Build
./autogen.sh
./configure
make

# opus_demo will be in the root directory
./opus_demo -h
```

**Generate test vectors with opus_demo:**

```bash
# Create silent audio
dd if=/dev/zero bs=2 count=160 of=silence_8khz_20ms.pcm

# Encode (creates raw packet)
./opus_demo -e voip 8000 1 12000 20 silence_8khz_20ms.pcm test.opus

# Decode (reference output)
./opus_demo -d 8000 1 test.opus reference.pcm

# test.opus is now a raw Opus packet (no container!)
```

## Alternative: Pre-generated Test Vectors

libopus includes test vectors in its repository:
- https://gitlab.xiph.org/xiph/opus/-/tree/master/tests

These could be downloaded and used directly.

## Running Tests

Once valid test vectors are available:

```bash
# Run integration tests
cargo test -p moosicbox_opus_native --test integration_tests

# Tests will:
# 1. Load test vectors from this directory
# 2. Decode packets with our decoder
# 3. Compare output to expected PCM using SNR
# 4. Assert SNR > 40 dB (SILK/CELT) or bit-exact (range decoder)
```

## Infrastructure Components

- **Loader:** `tests/test_vectors/mod.rs` - Loads test vectors, calculates SNR
- **Tests:** `tests/integration_tests.rs` - Runs decoder against all vectors
- **Generation:** `scripts/generate_vectors.sh` - Creates vectors (needs opus_demo)
- **Synthetic:** `scripts/create_synthetic_vectors.py` - Minimal packets for testing infrastructure

## Next Steps

1. Build `opus_demo` from libopus source
2. Update `scripts/generate_vectors.sh` to use `opus_demo` instead of `opusenc`
3. Generate comprehensive test vector set
4. Verify all tests pass with SNR > 40 dB
5. Check in test vectors to git for reproducible testing
