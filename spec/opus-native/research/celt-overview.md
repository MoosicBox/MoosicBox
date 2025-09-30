# CELT Decoder Overview

## Overview

CELT (Constrained Energy Lapped Transform) is the music-optimized component of Opus, designed for high quality audio at various bitrates.

## RFC 6716 References

- Section 4.3: CELT Decoder
- Section 4.3.1: Transient Decoding
- Section 4.3.2: Energy Envelope Decoding
- Section 4.3.7: Inverse MDCT

## Key Concepts

### MDCT (Modified Discrete Cosine Transform)

Transform coding like MP3/AAC:
- Converts time-domain signal to frequency domain
- Overlapping windows with 50% overlap
- Inverse MDCT reconstructs time-domain signal
- Overlap-add combines frames

### PVQ (Pyramid Vector Quantization)

Quantizes frequency coefficients:
- Allocates pulses across frequency bands
- Preserves energy per band
- Efficient encoding of spectral shape

### Energy Envelope

Coarse + fine energy quantization:
- Each frequency band has energy level
- Coarse quantization uses prediction
- Fine quantization adds precision

## Decoder Pipeline

```
Compressed Bitstream
      ↓
[Range Decoder] - Extract symbols
      ↓
[Transient Detection] - Short/long window selection
      ↓
[Coarse Energy] - Band energy levels (predicted)
      ↓
[Fine Energy] - Additional energy precision
      ↓
[Bit Allocation] - Determine bits per band
      ↓
[PVQ Decoding] - Decode spectral shape per band
      ↓
[Spreading] - Adjust coefficients
      ↓
[Denormalization] - Apply energy envelope
      ↓
[Inverse MDCT] - Transform to time domain
      ↓
[Post-filter] - Optional smoothing
      ↓
[De-emphasis] - Reverse pre-emphasis
      ↓
PCM Output
```

## Major Components

### Transient Decoding (RFC 4.3.1)

- Detects sudden attacks in audio
- Switches to shorter MDCT windows
- Single bit per frame

### Energy Envelope (RFC 4.3.2)

- Coarse energy: Predicted from previous frame
- Fine energy: 1-7 bits per band
- Log-domain encoding

### Bit Allocation (RFC 4.3.3)

Most complex part:
- Dynamic allocation based on available bits
- Considers band importance (psychoacoustic model)
- Iterative algorithm finds optimal distribution
- Must match encoder's allocation exactly

### PVQ Decoding (RFC 4.3.4)

- Decodes pulse positions per band
- Combinatorial encoding (very efficient)
- Preserves unit vector property

### Inverse MDCT (RFC 4.3.7)

- Requires bit-exact FFT-like computation
- Window functions applied
- Overlap-add with previous frame
- State maintained between frames

## Sample Rates

CELT supports:
- 16 kHz (wideband)
- 24 kHz (super-wideband)
- 48 kHz (fullband)

## Frame Sizes

Multiple frame durations:
- 2.5 ms, 5 ms, 10 ms, 20 ms

Smaller frames = lower latency but less compression efficiency.

## Implementation Challenges

1. **Bit Allocation**: Must match encoder exactly, complex algorithm
2. **MDCT**: Requires careful implementation for bit-exact results
3. **PVQ**: Combinatorial math can overflow if not careful
4. **Overlap-Add**: Must maintain state between frames correctly
5. **Anti-Collapse**: Prevents decoder artifacts (RFC 4.3.5)

## Test Strategy

- Test each component in isolation
- Test with real CELT frames from conformance suite
- Verify bit-exact output against libopus
- Test all sample rates and frame sizes
- Test transient detection edge cases

## References

- RFC 6716 Section 4.3
- CELT codec specification
- "The CELT ultra-low delay audio codec" - Valin et al.
- libopus CELT implementation
