# SILK Decoder Overview

## Overview

SILK (Skype Low-Complexity Internet Codec) is the speech-optimized component of Opus, designed for low bitrates and narrowband/wideband speech.

## RFC 6716 References

- Section 4.2: SILK Decoder
- Section 4.2.1: SILK Decoder Modules
- Section 4.2.2: LP Layer Organization
- Section 4.2.7: SILK Frame Contents

## Key Concepts

### Linear Prediction (LP)

SILK uses Linear Predictive Coding (LPC) to model the vocal tract:
- Predicts current sample from previous samples
- Residual (difference) is encoded efficiently
- Filters are represented as Line Spectral Frequencies (LSF)

### Long-Term Prediction (LTP)

Models pitch/voicing:
- Predicts current sample from samples ~one pitch period ago
- Reduces bitrate for voiced speech significantly
- Pitch lag and filter coefficients encoded

### Sample Rates

SILK supports:
- 8 kHz (narrowband)
- 12 kHz (mediumband)
- 16 kHz (wideband)
- 24 kHz (super-wideband)

Output is resampled to final rate if needed.

## Decoder Pipeline

```
Compressed Bitstream
      ↓
[Range Decoder] - Extract symbols using entropy coding
      ↓
[Header Parsing] - Frame type, gains, flags
      ↓
[LSF Decoding] - Line Spectral Frequencies (2 stages)
      ↓
[LSF → LPC] - Convert to LPC filter coefficients
      ↓
[LTP Params] - Pitch lag, filter coefficients, scaling
      ↓
[Excitation] - Decode residual signal (pulse positions + signs)
      ↓
[LTP Synthesis] - Apply long-term prediction
      ↓
[LPC Synthesis] - Apply linear prediction filter
      ↓
[Resampling] - Convert to output sample rate
      ↓
PCM Output
```

## Major Components

### LSF/LPC Decoding (RFC 4.2.7.5)

Most complex part:
- Two-stage vector quantization
- Large codebooks (~2000 lines of tables)
- Interpolation between frames
- Stabilization to prevent instability
- Conversion from LSF to LPC coefficients
- Limiting to prevent numerical issues

### LTP Decoding (RFC 4.2.7.6)

- Pitch lag decoding (4-257 samples)
- Filter coefficient selection from codebooks
- Scaling parameter

### Excitation Decoding (RFC 4.2.7.8)

- Rate level determines signal shaping
- Pulses positioned using combinatorial encoding
- LSB decoding for additional precision
- Sign decoding

### Synthesis

- LTP filter: `output[n] = excitation[n] + Σ(coeff[i] * output[n - lag + i])`
- LPC filter: `output[n] = ltp_output[n] + Σ(lpc[i] * output[n - i - 1])`

## Stereo Handling

- Mid/side coding option (RFC 4.2.7.2)
- Stereo prediction weights (RFC 4.2.7.1)
- Unmixing after decoding (RFC 4.2.8)

## Implementation Challenges

1. **LSF Codebooks**: Large lookup tables must be embedded
2. **Interpolation**: Careful floating-point or fixed-point math
3. **Filter Stability**: LSF/LPC conversion must ensure stable filters
4. **Resampling**: High-quality resampler needed

## Test Strategy

- Test each component in isolation
- Test with real SILK frames from conformance suite
- Verify bit-exact output against libopus
- Test all sample rates and configurations
- Test stereo decoding

## References

- RFC 6716 Section 4.2
- SILK codec specification (Skype)
- libopus SILK implementation
