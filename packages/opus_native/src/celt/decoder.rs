use crate::error::{Error, Result};
use crate::range::RangeDecoder;
use crate::{Channels, SampleRate};

use super::constants::{
    CELT_BINS_2_5MS, CELT_BINS_5MS, CELT_BINS_10MS, CELT_BINS_20MS, CELT_INTRA_PDF, CELT_NUM_BANDS,
    CELT_SILENCE_PDF, CELT_TRANSIENT_PDF,
};

/// CELT decoder state (RFC Section 4.3)
pub struct CeltState {
    /// Previous frame's final energy per band (Q8 format)
    pub prev_energy: [i16; CELT_NUM_BANDS],

    /// Post-filter state (if enabled)
    pub post_filter_state: Option<PostFilterState>,

    /// Previous frame's MDCT output for overlap-add
    pub overlap_buffer: Vec<f32>,

    /// Anti-collapse processing state
    pub anti_collapse_state: AntiCollapseState,
}

/// Post-filter state (RFC Section 4.3.7.1)
#[derive(Debug, Clone)]
pub struct PostFilterState {
    /// Previous pitch period
    #[allow(dead_code)]
    pub prev_period: u16,

    /// Previous pitch gain
    #[allow(dead_code)]
    pub prev_gain: u8,

    /// Filter memory
    #[allow(dead_code)]
    pub memory: Vec<f32>,
}

/// Anti-collapse state (RFC Section 4.3.5)
#[derive(Debug, Clone)]
pub struct AntiCollapseState {
    /// Seed for random number generator
    pub seed: u32,
}

impl CeltState {
    #[must_use]
    pub fn new(frame_size: usize, channels: usize) -> Self {
        Self {
            prev_energy: [0; CELT_NUM_BANDS],
            post_filter_state: None,
            overlap_buffer: vec![0.0; frame_size * channels],
            anti_collapse_state: AntiCollapseState { seed: 0 },
        }
    }

    /// Resets decoder state (for packet loss recovery)
    pub fn reset(&mut self) {
        self.prev_energy.fill(0);
        self.post_filter_state = None;
        self.overlap_buffer.fill(0.0);
        self.anti_collapse_state.seed = 0;
    }
}

pub struct CeltDecoder {
    sample_rate: SampleRate,
    #[allow(dead_code)]
    channels: Channels,
    frame_size: usize, // In samples
    state: CeltState,
}

impl CeltDecoder {
    /// Creates a new CELT decoder.
    ///
    /// # Errors
    ///
    /// * Returns an error if `frame_size` is invalid for the given `sample_rate`.
    pub fn new(sample_rate: SampleRate, channels: Channels, frame_size: usize) -> Result<Self> {
        // Validate frame size based on sample rate (RFC Section 2)
        // CELT supports 2.5/5/10/20 ms frames
        let valid_frame_sizes = match sample_rate {
            SampleRate::Hz8000 => vec![20, 40, 80, 160],
            SampleRate::Hz12000 => vec![30, 60, 120, 240],
            SampleRate::Hz16000 => vec![40, 80, 160, 320],
            SampleRate::Hz24000 => vec![60, 120, 240, 480],
            SampleRate::Hz48000 => vec![120, 240, 480, 960],
        };

        if !valid_frame_sizes.contains(&frame_size) {
            return Err(Error::CeltDecoder(format!(
                "invalid frame size {frame_size} for sample rate {sample_rate:?}"
            )));
        }

        let num_channels = match channels {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        };

        Ok(Self {
            sample_rate,
            channels,
            frame_size,
            state: CeltState::new(frame_size, num_channels),
        })
    }

    /// Resets decoder state
    pub fn reset(&mut self) {
        self.state.reset();
    }

    /// Decodes silence flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_silence(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf_u16(CELT_SILENCE_PDF, 15)?;
        Ok(value == 1)
    }

    /// Decodes post-filter flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_post_filter(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        range_decoder.ec_dec_bit_logp(1)
    }

    /// Decodes transient flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_transient(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf(CELT_TRANSIENT_PDF, 8)?;
        Ok(value == 1)
    }

    /// Decodes intra flag (RFC Table 56)
    ///
    /// # Errors
    ///
    /// Returns an error if range decoding fails.
    pub fn decode_intra(&self, range_decoder: &mut RangeDecoder) -> Result<bool> {
        let value = range_decoder.ec_dec_icdf(CELT_INTRA_PDF, 8)?;
        Ok(value == 1)
    }

    /// Returns frame duration in milliseconds
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn frame_duration_ms(&self) -> f32 {
        let sample_rate_f32 = self.sample_rate as u32 as f32;
        (self.frame_size as f32 * 1000.0) / sample_rate_f32
    }

    /// Returns MDCT bins per band for this frame size
    #[must_use]
    pub fn bins_per_band(&self) -> &'static [u8; CELT_NUM_BANDS] {
        let duration_ms = self.frame_duration_ms();
        if (duration_ms - 2.5).abs() < 0.1 {
            &CELT_BINS_2_5MS
        } else if (duration_ms - 5.0).abs() < 0.1 {
            &CELT_BINS_5MS
        } else if (duration_ms - 10.0).abs() < 0.1 {
            &CELT_BINS_10MS
        } else {
            &CELT_BINS_20MS
        }
    }

    /// Returns frame duration index (0=2.5ms, 1=5ms, 2=10ms, 3=20ms)
    #[must_use]
    fn frame_duration_index(&self) -> usize {
        let duration = self.frame_duration_ms();
        if (duration - 2.5).abs() < 0.1 {
            0
        } else if (duration - 5.0).abs() < 0.1 {
            1
        } else if (duration - 10.0).abs() < 0.1 {
            2
        } else {
            3
        }
    }

    /// Decodes coarse energy for all bands
    ///
    /// RFC 6716 Section 4.3.2.1 (lines 6034-6077)
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `intra_flag` - Whether this is an intra frame (from `decode_intra()`)
    ///
    /// # Returns
    ///
    /// Array of coarse energy values (Q8 format, base-2 log domain)
    ///
    /// # Errors
    ///
    /// * Returns error if Laplace decoding fails
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn decode_coarse_energy(
        &mut self,
        range_decoder: &mut RangeDecoder,
        intra_flag: bool,
    ) -> Result<[i16; CELT_NUM_BANDS]> {
        use super::constants::{
            ENERGY_ALPHA_INTER, ENERGY_BETA_INTER, ENERGY_BETA_INTRA, ENERGY_PROB_MODEL,
        };

        let mut coarse_energy = [0_i16; CELT_NUM_BANDS];
        let mut prev = 0.0_f32;

        let (alpha, beta) = if intra_flag {
            (0.0, ENERGY_BETA_INTRA)
        } else {
            let frame_idx = self.frame_duration_index();
            (ENERGY_ALPHA_INTER[frame_idx], ENERGY_BETA_INTER[frame_idx])
        };

        let frame_idx = self.frame_duration_index();
        let prob_model = &ENERGY_PROB_MODEL[frame_idx][usize::from(intra_flag)];

        #[allow(clippy::needless_range_loop)]
        for band in 0..CELT_NUM_BANDS {
            let time_pred = if intra_flag || self.state.prev_energy[band] == 0 {
                0.0
            } else {
                alpha * f32::from(self.state.prev_energy[band])
            };

            let freq_pred = prev;

            let prediction = time_pred + freq_pred;

            let pi = 2 * band.min(20);
            let fs = u32::from(prob_model[pi]) << 7;
            let decay = u32::from(prob_model[pi + 1]) << 6;

            let error = range_decoder.ec_laplace_decode(fs, decay)?;

            #[allow(clippy::cast_precision_loss)]
            let q = error as f32 * 6.0;
            let raw_energy = prediction + q;

            coarse_energy[band] = raw_energy.clamp(-128.0, 127.0) as i16;

            prev = beta.mul_add(-q, prev + q);
        }

        Ok(coarse_energy)
    }

    /// Decodes fine energy quantization
    ///
    /// RFC 6716 Section 4.3.2.2 (lines 6079-6087)
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `coarse_energy` - Coarse energy from `decode_coarse_energy()`
    /// * `fine_bits` - Bits allocated per band (from Section 4.3.3)
    ///
    /// # Returns
    ///
    /// Refined energy values (Q8 format)
    ///
    /// # Errors
    ///
    /// * Returns error if range decoding fails
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss
    )]
    pub fn decode_fine_energy(
        &self,
        range_decoder: &mut RangeDecoder,
        coarse_energy: &[i16; CELT_NUM_BANDS],
        fine_bits: &[u8; CELT_NUM_BANDS],
    ) -> Result<[i16; CELT_NUM_BANDS]> {
        let mut refined_energy = *coarse_energy;

        for band in 0..CELT_NUM_BANDS {
            let bits = fine_bits[band];

            if bits == 0 {
                continue;
            }

            let ft = 1_u32 << bits;
            let f = range_decoder.ec_dec_uint(ft)?;

            let correction = ((f as f32 + 0.5) / ft as f32) - 0.5;

            let correction_q8 = (correction * 256.0) as i16;

            refined_energy[band] = refined_energy[band].saturating_add(correction_q8);
        }

        Ok(refined_energy)
    }

    /// Decodes final fine energy allocation from unused bits
    ///
    /// RFC 6716 Section 4.3.2.2 (lines 6089-6099)
    ///
    /// # Arguments
    ///
    /// * `range_decoder` - Range decoder state
    /// * `fine_energy` - Energy after fine quantization
    /// * `priorities` - Priority (0 or 1) per band (from allocation)
    /// * `unused_bits` - Remaining bits after all decoding
    ///
    /// # Returns
    ///
    /// Final energy values with extra refinement
    ///
    /// # Errors
    ///
    /// * Returns error if range decoding fails
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn decode_final_energy(
        &self,
        range_decoder: &mut RangeDecoder,
        fine_energy: &[i16; CELT_NUM_BANDS],
        priorities: &[u8; CELT_NUM_BANDS],
        mut unused_bits: u32,
    ) -> Result<[i16; CELT_NUM_BANDS]> {
        let mut final_energy = *fine_energy;
        let channels = match self.channels {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        };

        for band in 0..CELT_NUM_BANDS {
            if priorities[band] == 0 && unused_bits >= channels {
                for _ in 0..channels {
                    if unused_bits == 0 {
                        break;
                    }

                    let bit = range_decoder.ec_dec_bit_logp(1)?;
                    let correction = if bit { 0.5 } else { -0.5 };
                    final_energy[band] =
                        final_energy[band].saturating_add((correction * 256.0) as i16);

                    unused_bits -= 1;
                }
            }
        }

        for band in 0..CELT_NUM_BANDS {
            if priorities[band] == 1 && unused_bits >= channels {
                for _ in 0..channels {
                    if unused_bits == 0 {
                        break;
                    }

                    let bit = range_decoder.ec_dec_bit_logp(1)?;
                    let correction = if bit { 0.5 } else { -0.5 };
                    final_energy[band] =
                        final_energy[band].saturating_add((correction * 256.0) as i16);

                    unused_bits -= 1;
                }
            }
        }

        Ok(final_energy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_celt_decoder_creation() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_frame_size_validation_48khz() {
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 120).is_ok()); // 2.5ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 240).is_ok()); // 5ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).is_ok()); // 10ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).is_ok()); // 20ms
        assert!(CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 100).is_err()); // invalid
    }

    #[test]
    fn test_frame_duration_calculation() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        assert!((decoder.frame_duration_ms() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_bins_per_band_10ms() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();
        let bins = decoder.bins_per_band();
        assert_eq!(bins[0], 4); // Band 0: 4 bins for 10ms
        assert_eq!(bins[20], 88); // Band 20: 88 bins for 10ms
    }

    #[test]
    fn test_state_initialization() {
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Stereo, 480).unwrap();
        assert_eq!(decoder.state.prev_energy.len(), CELT_NUM_BANDS);
        assert_eq!(decoder.state.overlap_buffer.len(), 480 * 2); // stereo
        assert!(decoder.state.post_filter_state.is_none());
    }

    #[test]
    fn test_state_reset() {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        // Modify state
        decoder.state.prev_energy[0] = 100;
        decoder.state.overlap_buffer[0] = 1.5;
        decoder.state.anti_collapse_state.seed = 42;

        // Reset
        decoder.reset();

        // Verify reset
        assert_eq!(decoder.state.prev_energy[0], 0);
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(decoder.state.overlap_buffer[0], 0.0);
        }
        assert_eq!(decoder.state.anti_collapse_state.seed, 0);
    }

    #[test]
    fn test_silence_flag_decoding() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_silence(&mut range_decoder);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transient_flag_decoding() {
        let data = vec![0x80, 0x00, 0x00, 0x00];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_transient(&mut range_decoder);
        assert!(result.is_ok());
        let _ = result.unwrap();
    }

    #[test]
    fn test_coarse_energy_intra() {
        let data = vec![0x55; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_coarse_energy(&mut range_decoder, true);
        assert!(result.is_ok());
        let energy = result.unwrap();
        assert_eq!(energy.len(), CELT_NUM_BANDS);
    }

    #[test]
    fn test_coarse_energy_inter() {
        let data = vec![0xAA; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        decoder.state.prev_energy[0] = 50;
        decoder.state.prev_energy[10] = 60;

        let result = decoder.decode_coarse_energy(&mut range_decoder, false);
        assert!(result.is_ok());
        let energy = result.unwrap();
        assert_eq!(energy.len(), CELT_NUM_BANDS);
    }

    #[test]
    fn test_coarse_energy_clamping() {
        let data = vec![0xFF; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let result = decoder.decode_coarse_energy(&mut range_decoder, true);
        assert!(result.is_ok());
        let energy = result.unwrap();

        for &e in &energy {
            assert!((-128..=127).contains(&e));
        }
    }

    #[test]
    fn test_fine_energy_no_bits() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let coarse_energy = [10_i16; CELT_NUM_BANDS];
        let fine_bits = [0_u8; CELT_NUM_BANDS];

        let result = decoder.decode_fine_energy(&mut range_decoder, &coarse_energy, &fine_bits);
        assert!(result.is_ok());
        let energy = result.unwrap();

        assert_eq!(energy, coarse_energy);
    }

    #[test]
    fn test_fine_energy_single_bit() {
        let data = vec![0xAA; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let coarse_energy = [10_i16; CELT_NUM_BANDS];
        let mut fine_bits = [0_u8; CELT_NUM_BANDS];
        fine_bits[0] = 1;

        let result = decoder.decode_fine_energy(&mut range_decoder, &coarse_energy, &fine_bits);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fine_energy_multiple_bits() {
        let data = vec![0x55; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let coarse_energy = [10_i16; CELT_NUM_BANDS];
        let fine_bits = [2_u8; CELT_NUM_BANDS];

        let result = decoder.decode_fine_energy(&mut range_decoder, &coarse_energy, &fine_bits);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_energy_priority_0() {
        let data = vec![0xFF; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let fine_energy = [10_i16; CELT_NUM_BANDS];
        let priorities = [0_u8; CELT_NUM_BANDS];

        let result = decoder.decode_final_energy(&mut range_decoder, &fine_energy, &priorities, 21);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_energy_both_priorities() {
        let data = vec![0xAA; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let fine_energy = [10_i16; CELT_NUM_BANDS];
        let mut priorities = [0_u8; CELT_NUM_BANDS];
        priorities[10] = 1;
        priorities[11] = 1;

        let result = decoder.decode_final_energy(&mut range_decoder, &fine_energy, &priorities, 30);
        assert!(result.is_ok());
    }

    #[test]
    fn test_final_energy_unused_bits_left() {
        let data = vec![0x00; 256];
        let mut range_decoder = RangeDecoder::new(&data).unwrap();
        let decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 480).unwrap();

        let fine_energy = [10_i16; CELT_NUM_BANDS];
        let priorities = [0_u8; CELT_NUM_BANDS];

        let result = decoder.decode_final_energy(&mut range_decoder, &fine_energy, &priorities, 5);
        assert!(result.is_ok());
    }
}
