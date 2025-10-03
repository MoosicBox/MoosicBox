# Implementation References

This document tracks all reference sources used during the implementation of `moosicbox_opus_native`.

## Specification

* **RFC 6716** - Definition of the Opus Audio Codec
  * URL: https://datatracker.ietf.org/doc/html/rfc6716
  * Local copy: `../../spec/opus/rfc6716.txt`

## Reference Implementation

* **Official Repository**: https://gitlab.xiph.org/xiph/opus
* **Commit Referenced**: `34bba701ae97c913de719b1f7c10686f62cddb15`
* **Date**: 2025-09-28
* **Verified**: 2025-10-02
* **License**: BSD 3-Clause

## Source File Mapping

### Range Decoder (Phase 1)

| Our File | Reference File | Lines | Function/Constants |
|----------|----------------|-------|-------------------|
| `src/range/decoder.rs` | `celt/entdec.c` | - | `ec_decode()`, `ec_dec_bit_logp()`, `ec_dec_icdf()`, `ec_dec_uint()` |
| `src/range/decoder.rs` | [`celt/laplace.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/laplace.c#L101-142) | L101-142 | `ec_laplace_decode()` |
| `src/range/decoder.rs` | [`celt/laplace.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/laplace.c#L38-42) | L38-42 | LAPLACE constants |

### CELT Decoder (Phase 4)

| Our File | Reference File | Lines | Function/Constants |
|----------|----------------|-------|-------------------|
| `src/celt/constants.rs` | `celt/bands.c` | - | Band frequency tables, bin counts |
| `src/celt/constants.rs` | [`celt/quant_bands.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L77-138) | L77-138 | `e_prob_model` |
| `src/celt/constants.rs` | [`celt/quant_bands.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L67-69) | L67-69 | `pred_coef`, `beta_coef`, `beta_intra` |
| `src/celt/decoder.rs` | [`celt/quant_bands.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L427-490) | L427-490 | `unquant_coarse_energy()` |
| `src/celt/decoder.rs` | [`celt/quant_bands.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L487) | L487 | **IIR filter** (critical) |
| `src/celt/decoder.rs` | [`celt/quant_bands.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L492-510) | L492-510 | `unquant_fine_energy()` |
| `src/celt/decoder.rs` | [`celt/quant_bands.c`](https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L512-539) | L512-539 | `unquant_energy_finalise()` |
| `src/celt/decoder.rs` | `celt/celt_decoder.c` | - | Main decoder structure and flow |

### SILK Decoder (Phases 2-3)

| Our File | Reference File | Lines | Function/Constants |
|----------|----------------|-------|-------------------|
| `src/silk/lsf_constants.rs` | `silk/tables_NLSF_CB_*.c` | - | LSF codebooks |
| `src/silk/ltp_constants.rs` | `silk/tables_LTP.c` | - | LTP filter tables |
| `src/silk/excitation_constants.rs` | `silk/tables_pulses_per_block.c` | - | Excitation parameters |

## Critical Implementation Notes

### Phase 4.2 - Energy Envelope Decoding

**Line L487** - IIR filter state update (Phase 4.2 bug fix):
```c
prev[c] = prev[c] + q - MULT16_32_Q15(beta,q);
```

**URL**: https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/quant_bands.c#L487

**Bug Fixed**: Initial implementation incorrectly computed frequency prediction as:
```rust
prev = beta * f32::from(coarse_energy[band]);  // ❌ WRONG
```

**Correct Implementation**:
```rust
prev = beta.mul_add(-q, prev + q);  // ✅ CORRECT: prev + q - beta*q
```

This implements the IIR filter component of the 2-D prediction filter specified in RFC 6716 lines 6055-6063:
```
A(z_l, z_b) = (1 - alpha*z_l^-1)*(1 - z_b^-1) / (1 - beta*z_b^-1)
```

## Verification Method

All constants and algorithms are verified against the reference implementation by:

1. **Direct source code inspection** (manual verification against GitLab URLs)
2. **Bit-exact value comparison** (for lookup tables)
3. **Formula matching** (for algorithmic components)
4. **Future**: RFC test vector validation (Phase 8)

## License Compatibility

* **xiph/opus**: BSD 3-Clause License
* **This implementation**: See workspace LICENSE file
* Both licenses are compatible for reference-based implementation

All constants extracted from xiph/opus are properly attributed and referenced.

---

**Last Updated**: 2025-10-02
**Verified Against Commit**: `34bba701ae97c913de719b1f7c10686f62cddb15`
