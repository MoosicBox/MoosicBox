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
        // Verify it returns a boolean
        let _ = result.unwrap();
    }
}
