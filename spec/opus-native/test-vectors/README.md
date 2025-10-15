# Opus Test Vectors

This directory contains test vectors for RFC 6716 conformance testing.

## Sources

Test vectors will be obtained from:

- RFC 6716 reference implementation (if available)
- Opus test suite (opus-tools test data)
- libopus test data
- Hand-crafted edge cases for specific scenarios

## Organization

```
test-vectors/
├── range-decoder/     # Range decoder test cases
│   ├── 001_simple.bin     # Simple symbol sequence
│   ├── 002_uniform.bin    # Uniform distribution
│   └── ...
├── silk/              # SILK decoder test cases
│   ├── narrowband/        # 8 kHz samples
│   ├── wideband/          # 16 kHz samples
│   └── ...
├── celt/              # CELT decoder test cases
│   ├── wideband/          # 16 kHz samples
│   ├── fullband/          # 48 kHz samples
│   └── ...
├── integration/       # End-to-end test cases
│   ├── speech/            # Speech samples (SILK mode)
│   ├── music/             # Music samples (CELT mode)
│   ├── hybrid/            # Hybrid mode samples
│   └── transitions/       # Mode switching scenarios
└── edge-cases/        # Edge cases and error conditions
    ├── truncated.bin      # Truncated packets
    ├── zero-length.bin    # Zero-length frames
    └── ...
```

## Test Vector Format

Each test vector consists of:

### Input File (`.opus` or `.bin`)

- Opus packet(s) in binary format
- Can be single packet or packet sequence

### Expected Output (`.pcm`)

- Raw PCM samples (signed 16-bit or 32-bit float)
- Little-endian byte order
- Interleaved for stereo

### Metadata File (`.json`)

```json
{
    "name": "Test case name",
    "description": "What this test validates",
    "rfc_section": "4.1.2",
    "sample_rate": 48000,
    "channels": 2,
    "frame_size": 960,
    "mode": "celt",
    "packet_count": 1,
    "notes": "Additional information"
}
```

## Usage in Tests

Test vectors are consumed by integration tests:

```rust
#[test]
fn test_rfc_vector_range_001() {
    let packet = include_bytes!("../test-vectors/range-decoder/001_simple.bin");
    let expected = include_bytes!("../test-vectors/range-decoder/001_simple.pcm");
    let metadata = include_str!("../test-vectors/range-decoder/001_simple.json");

    // Parse metadata
    let meta: TestMetadata = serde_json::from_str(metadata).unwrap();

    // Decode packet
    let mut decoder = Decoder::new(meta.sample_rate(), meta.channels()).unwrap();
    let mut output = vec![0i16; meta.frame_size() * meta.channels() as usize];
    let decoded = decoder.decode(Some(packet), &mut output, false).unwrap();

    // Compare with expected output
    let expected_samples: &[i16] = bytemuck::cast_slice(expected);
    assert_eq!(&output[..decoded], &expected_samples[..decoded]);
}
```

## Creating Test Vectors

### From libopus

Use libopus encoder to create reference packets:

```c
#include <opus.h>

// Create encoder
OpusEncoder *enc = opus_encoder_create(48000, 2, OPUS_APPLICATION_AUDIO, &error);

// Encode audio
unsigned char packet[1275];
int len = opus_encode(enc, pcm_input, 960, packet, sizeof(packet));

// Save packet to file for test vector
```

### From Opus Tools

Use `opusenc` and `opusdec` from opus-tools:

```bash
# Encode audio file to opus
opusenc input.wav output.opus

# Extract packets for testing
# (May need custom tool to extract individual packets)
```

### Hand-Crafted Edge Cases

For testing specific error conditions:

```rust
// Create packet with specific structure
let mut packet = vec![
    0x03,  // TOC: CELT mode, 20ms frame
    0x84,  // Frame count: 4, CBR, padding
    10,    // Padding size
    // ... frame data ...
];
```

## Validation Strategy

1. **Range Decoder**: Test with known entropy-coded sequences
2. **SILK**: Test all sample rates (8/12/16/24 kHz)
3. **CELT**: Test all sample rates (16/24/48 kHz) and frame sizes
4. **Mode Transitions**: Test SILK→CELT, CELT→SILK, etc.
5. **Error Handling**: Test malformed packets, truncated data
6. **Edge Cases**: Test boundary conditions (zero samples, max values)

## Test Coverage Goals

- [ ] Range decoder: Basic symbol decoding
- [ ] Range decoder: Binary symbols
- [ ] Range decoder: Raw bits
- [ ] Range decoder: Uniform integers
- [ ] SILK: Narrowband (8 kHz)
- [ ] SILK: Wideband (16 kHz)
- [ ] SILK: Super-wideband (24 kHz)
- [ ] SILK: Stereo decoding
- [ ] CELT: Wideband (16 kHz)
- [ ] CELT: Super-wideband (24 kHz)
- [ ] CELT: Fullband (48 kHz)
- [ ] CELT: Stereo decoding
- [ ] CELT: Transient frames
- [ ] Hybrid: SILK+CELT combination
- [ ] Mode switching: All transitions
- [ ] PLC: Packet loss scenarios
- [ ] Error cases: All RFC validation rules

## Adding New Test Vectors

1. Create binary packet file (`.bin` or `.opus`)
2. Generate expected PCM output (using libopus or known-good decoder)
3. Create metadata JSON file
4. Add test case to appropriate test file
5. Verify test passes with reference implementation
6. Document what the test validates

## Notes

- Test vectors should be small (< 100 KB each) for fast tests
- Use descriptive names indicating what is tested
- Document source of test vector (RFC example, libopus, hand-crafted)
- Include both success and failure test cases
- Test vectors are binary files, not checked into git by default (use git-lfs if needed)
