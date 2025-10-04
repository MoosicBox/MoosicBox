# Future Work: Stereo Support for CELT Decoder

## Overview

Current CELT decoder implementation supports **mono (C=1) only**. This document tracks required changes for stereo (C=2) support, planned for Phase 5 (Stereo Intensity/Dual Stereo) and Phase 6 (Mid-Side Stereo).

## Affected Components

### 1. Anti-Collapse Processing (Phase 4.6.1)

**Current Implementation:** `packages/opus_native/src/celt/decoder.rs:1255-1420`

**Required Changes:**

* **Collapse masks indexing:**
  * Current: `collapse_masks[band_idx]` (mono, C=1)
  * Stereo: `collapse_masks[band_idx * C + channel]` (C=2 for stereo)
  * Each channel has independent collapse detection per RFC 6716

* **Energy comparison for stereo→mono playback:**
  * Current: Uses `current_energy[band_idx]` directly
  * Stereo: Must use `MAX(energy[ch0], energy[ch1])` per RFC line 6727
  * Prevents artifacts when stereo decoded as mono

* **Band structure:**
  * Current: `bands: &mut [Vec<f32>]` (flat array, mono)
  * Stereo: `bands: &mut [[Vec<f32>; C]]` or `&mut [Vec<f32>]` with `[i*C+c]` indexing
  * Each channel requires separate band arrays

* **Per-channel PRNG:**
  * Current: Single `AntiCollapseState` in `CeltState`
  * Stereo: One `AntiCollapseState` per channel
  * Ensures independent noise injection per channel

**Implementation Checklist:**
- [ ] Add channel parameter to `apply_anti_collapse()`
- [ ] Update collapse_masks indexing: `[i*C+c]`
- [ ] Implement MAX energy logic for stereo→mono
- [ ] Add per-channel AntiCollapseState array
- [ ] Update band structure for stereo
- [ ] Add stereo anti-collapse tests

### 2. PVQ Decoder (Phase 4.4)

**Current Implementation:** `packages/opus_native/src/celt/decoder.rs` (PVQ section)

**Required Changes:**

* **Collapse masks array:**
  * Current: `vec![0xFF_u8; CELT_NUM_BANDS]` (mono)
  * Stereo: `vec![0xFF_u8; CELT_NUM_BANDS * C]`
  * Size must match `[i*C+c]` indexing requirement

* **Per-channel decoding:**
  * Stereo requires separate PVQ decode for each channel
  * Intensity stereo: Some bands share single PVQ decode
  * Dual stereo: Independent PVQ per channel

**Implementation Checklist:**
- [ ] Expand collapse_masks array size for stereo
- [ ] Add channel loop for PVQ decoding
- [ ] Implement intensity stereo band sharing
- [ ] Add dual stereo independent decoding

### 3. Energy Decoding (Phase 4.2)

**Current Implementation:** Energy arrays are mono-sized

**Required Changes:**

* **Energy arrays:**
  * Current: `[i16; CELT_NUM_BANDS]`
  * Stereo: `[i16; CELT_NUM_BANDS * C]` or per-channel arrays
  * Must support `[i*C+c]` indexing

* **Coarse/fine energy:**
  * Stereo requires per-channel energy decode
  * Mid-side stereo: Side channel has different probability distribution

**Implementation Checklist:**
- [ ] Expand energy array dimensions
- [ ] Add per-channel energy decode
- [ ] Implement mid-side energy handling

### 4. Denormalization (Phase 4.6.2)

**Current Implementation:** Not yet implemented

**Required Changes:**

* **Band scaling:**
  * Must scale each channel independently
  * Energy indexing: `energy[i*C+c]`

**Implementation Checklist:**
- [ ] Add channel parameter to denormalization
- [ ] Update energy indexing for stereo

### 5. Inverse MDCT & Windowing (Phase 4.6.3)

**Current Implementation:** Not yet implemented

**Required Changes:**

* **Per-channel iMDCT:**
  * Each channel requires separate iMDCT
  * Overlap-add state per channel

**Implementation Checklist:**
- [ ] Add per-channel iMDCT processing
- [ ] Add per-channel overlap-add buffers

## Phase Dependencies

**Phase 5: Stereo Intensity/Dual Stereo (RFC Section 4.3.7)**
- Provides stereo mode signaling
- Defines intensity stereo band ranges
- Defines dual stereo independent processing

**Phase 6: Mid-Side Stereo (RFC Section 4.3.8)**
- Provides mid-side transformation
- Defines side channel energy handling
- Requires stereo band structure from Phase 5

## Testing Requirements

**Stereo Anti-Collapse Tests:**
- [ ] Test with both channels collapsed
- [ ] Test with one channel collapsed
- [ ] Test with neither channel collapsed
- [ ] Test stereo→mono energy MAX logic
- [ ] Test independent PRNG per channel

**Stereo Integration Tests:**
- [ ] End-to-end stereo frame decode
- [ ] Intensity stereo mode
- [ ] Dual stereo mode
- [ ] Mid-side stereo mode

## libopus Reference

**Key files for stereo implementation:**
* `celt/bands.c:anti_collapse()` - stereo indexing pattern
* `celt/celt_decoder.c:celt_decode_lost()` - stereo structure
* `celt/quant_bands.c` - stereo energy handling

**Critical indexing patterns:**
```c
// libopus stereo indexing
collapse_masks[i*C+c]      // Collapse detection per channel
oldLogE[i*C+c]             // Energy per channel per band
X[(i*N+j)*C+c]             // Frequency data interleaving
```

## Implementation Priority

**High Priority (Phase 5 Blockers):**
1. Collapse masks indexing fix
2. Band structure expansion for stereo
3. Per-channel energy arrays

**Medium Priority (Phase 6 Enhancements):**
1. Mid-side energy handling
2. Stereo→mono playback optimization

**Low Priority (Future Optimization):**
1. SIMD-optimized per-channel processing
2. Cache-friendly stereo data layout

## Notes

* **Zero compromises required:** All changes are additive
* **Mono compatibility:** Mono code paths remain unchanged (C=1 specialization)
* **RFC compliance:** All stereo changes directly from RFC 6716 specification
* **libopus match:** Stereo indexing patterns match libopus exactly
