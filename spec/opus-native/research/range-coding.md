# Range Coding in Opus

## Overview

Range coding is an entropy coding method used in Opus for compressing the bitstream. It's similar to arithmetic coding but uses integer arithmetic for efficiency.

## RFC 6716 References

- Section 4.1: Range Decoder
- Section 4.1.1: Range Decoder Initialization
- Section 4.1.2: Decoding Symbols
- Section 4.1.2.1: Renormalization

## Key Concepts

### State Machine

The range decoder maintains:

- `value`: Current position in the range (u32)
- `range`: Size of the current range (u32)
- `position`: Current byte position in input buffer
- `total_bits`: Number of bits consumed

### Symbol Decoding

Symbols are decoded using probability models (frequency tables). The decoder:

1. Computes a scaled value within the current range
2. Looks up which symbol corresponds to that value
3. Updates the range to the symbol's probability range
4. Renormalizes if the range becomes too small

### Renormalization

When the range becomes smaller than a threshold (128), the decoder:

1. Shifts the range left by 8 bits
2. Reads the next byte from the input buffer
3. Updates the value accordingly

## Implementation Notes

### Initialization

- Range starts at 128
- First bytes are loaded into value
- Buffer must have at least 2 bytes

### Binary Symbols

- RFC 4.1.3.1 provides optimized path for binary (0/1) symbols
- Uses simple threshold comparison

### Raw Bits

- RFC 4.1.4 provides method to extract raw bits
- Used when data is already uniform (no compression benefit)

## Algorithm Pseudocode

```
function decode_symbol(frequencies):
    scaled = (value * total_frequency) / range

    symbol = lookup_symbol_from_scaled(scaled, frequencies)

    low = frequencies[symbol].low
    high = frequencies[symbol].high

    range = range * (high - low) / total_frequency
    value = value - low * range / total_frequency

    renormalize_if_needed()

    return symbol

function renormalize():
    while range < 128:
        range = range << 8
        value = (value << 8) | read_next_byte()
```

## Test Strategy

- Test initialization with various buffer sizes
- Test symbol decoding with known frequency tables
- Test renormalization logic
- Test bit usage tracking accuracy
- Compare against RFC test vectors

## References

- RFC 6716 Section 4.1
- "Introduction to Arithmetic Coding" - Witten, Neal, Cleary (1987)
- libopus reference implementation
