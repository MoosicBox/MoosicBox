# MDCT Implementation Notes

## Overview

The Modified Discrete Cosine Transform (MDCT) is a key component of the CELT decoder. It must be implemented with bit-exact accuracy to match the RFC specification.

## RFC 6716 References

- Section 4.3.7: Inverse MDCT
- Section 4.3.7.1: Post-Filter
- Section 4.3.7.2: De-emphasis

## Mathematical Background

### MDCT Properties

- Transform length: N (e.g., 120, 240, 480, 960)
- Input: N frequency coefficients
- Output: N time-domain samples
- 50% overlap with adjacent frames
- Perfect reconstruction when overlapped-added

### IMDCT Formula

For N coefficients X[k], output samples x[n]:

```
x[n] = Σ(k=0 to N-1) X[k] * cos(π/N * (n + 0.5 + N/2) * (k + 0.5))
```

Where:
- n: time index (0 to N-1)
- k: frequency index (0 to N-1)

### Windowing

After IMDCT, apply window function:
- Sine window for smooth overlap
- Formula: `w[n] = sin(π/N * (n + 0.5))`

### Overlap-Add

Current frame output overlaps with previous frame:
```
output[n] = current_frame[n] * window[n] + previous_frame[n + N/2] * window[n + N/2]
```

## Implementation Approaches

### 1. Direct Computation (Naive)

```rust
for n in 0..N {
    let mut sum = 0.0;
    for k in 0..N {
        let angle = PI / N as f32 * (n as f32 + 0.5 + N as f32 / 2.0) * (k as f32 + 0.5);
        sum += X[k] * angle.cos();
    }
    x[n] = sum;
}
```

**Pros**: Simple, obviously correct
**Cons**: O(N²) complexity, too slow

### 2. FFT-Based (Efficient)

MDCT can be computed using FFT:
1. Pre-rotation of input
2. N-point FFT
3. Post-rotation of output

**Pros**: O(N log N) complexity
**Cons**: More complex, requires careful implementation

### 3. Library-Based

Use existing FFT library (e.g., rustfft):
- Wrap in MDCT interface
- Handle pre/post-rotation
- Ensure bit-exact results

## Bit-Exact Requirements

### Floating-Point Precision

- Use f32 (32-bit float) to match libopus
- Be aware of rounding differences across platforms
- May need to test on multiple architectures

### Trig Function Accuracy

- cos() implementation varies by platform
- May need to use lookup tables for bit-exact results
- Consider fixed-point arithmetic alternative

### Window Function

Pre-compute and store:
```rust
const WINDOW_120: [f32; 120] = [/* precomputed values */];
const WINDOW_240: [f32; 240] = [/* precomputed values */];
// ... etc for each frame size
```

## State Management

### Overlap Buffer

Decoder must maintain:
```rust
struct CeltDecoder {
    overlap_buffer: Vec<f32>,  // Size: N/2
    // ... other state
}
```

### Initialization

First frame:
- No previous frame to overlap with
- Use silence or fade-in

### Reset

On `reset_state()`:
- Clear overlap buffer to zeros
- Reset any filter state

## Implementation Strategy

### Phase 1: Direct Implementation

1. Implement naive O(N²) version first
2. Verify correctness with test vectors
3. Get bit-exact results

### Phase 2: Optimization

1. Implement FFT-based version
2. Verify matches naive version
3. Benchmark performance

### Phase 3: Verification

1. Compare against libopus output
2. Test all frame sizes (120, 240, 480, 960)
3. Test transient cases (window switching)

## Testing Strategy

### Unit Tests

- Test IMDCT with known input/output pairs
- Test window function values
- Test overlap-add logic
- Test state preservation across frames

### Integration Tests

- Feed real CELT frames
- Compare output with libopus sample-by-sample
- Allow small floating-point tolerance (< 0.0001)

### Edge Cases

- First frame (no previous overlap)
- Reset state mid-stream
- Window size changes (transients)
- Zero input (silence)

## References

- RFC 6716 Section 4.3.7
- "Introduction to Data Compression" - Sayood (MDCT chapter)
- "Digital Signal Processing" - Oppenheim & Schafer
- libopus MDCT implementation
- rustfft documentation
