//! CELT transition primitives; Opus mode scheduling belongs to the packet decoder.
use super::{CeltDecoder, DecodedFrame, RangeDecoder, Result, transform};
use crate::{Channels, Error, SampleRate};

impl CeltDecoder {
    /// Decode a separately coded 5-ms redundancy payload (no TOC) at 48 kHz.
    ///
    /// Uses bands 0..`end_band`, preserving predictor, TDAC, postfilter and PLC
    /// history. Does not reset: the packet decoder decides the ordering/reset.
    /// Returns interleaved Q12, without deemphasis or output-rate conversion.
    /// Restores the previous frame size and band configuration on success/error;
    /// synthesis history is committed, and decode errors need not roll it back.
    /// Coded/output channels and spectral output rate must already be configured.
    ///
    /// # Errors
    /// * Rejects non-48-kHz construction, empty/oversized payloads, or end bands
    ///   other than 13, 17, 19 and 21, before changing state.
    /// * Propagates entropy and CELT reconstruction errors.
    pub fn decode_transition_redundancy(
        &mut self,
        payload: &[u8],
        end_band: usize,
    ) -> Result<DecodedFrame> {
        if self.sample_rate != SampleRate::Hz48000
            || !(1..=1275).contains(&payload.len())
            || !matches!(end_band, 13 | 17 | 19 | 21)
        {
            return Err(Error::CeltDecoder(
                "invalid transition redundancy configuration".into(),
            ));
        }
        let mut entropy = RangeDecoder::new(payload)?;
        let saved = (self.frame_size, self.start_band, self.end_band);
        self.frame_size = 240;
        self.start_band = 0;
        self.end_band = end_band;
        let result = self.decode_celt_frame(&mut entropy, payload.len());
        (self.frame_size, self.start_band, self.end_band) = saved;
        result
    }

    /// Drain 2.5 ms of CELT overlap with the reference `[0xff, 0xff]` frame.
    ///
    /// Returns 120 samples/channel in Q12; no deemphasis or reset. Restores the
    /// caller's frame size/bands but commits synthesis and energy/PLC history.
    /// Output channels/rate must be configured before calling. The packet decoder
    /// must reset its external deemphasis memory whenever it resets CELT.
    ///
    /// # Errors
    /// * Rejects construction rates other than 48 kHz without changing state.
    /// * Propagates entropy/synthesis errors.
    pub fn decode_transition_silence(&mut self) -> Result<DecodedFrame> {
        if self.sample_rate != SampleRate::Hz48000 {
            return Err(Error::CeltDecoder(
                "transition silence requires 48-kHz synthesis".into(),
            ));
        }
        let mut entropy = RangeDecoder::new(&[0xff, 0xff])?;
        let saved = (self.frame_size, self.start_band, self.end_band);
        self.frame_size = 120;
        self.start_band = 0;
        let result = self.decode_celt_frame(&mut entropy, 2);
        (self.frame_size, self.start_band, self.end_band) = saved;
        result
    }

    pub(super) fn synthesize_silence(&mut self, entropy: &RangeDecoder) -> DecodedFrame {
        let coded = self.channels as usize;
        let channels = self.synthesis_channels as usize;
        let energy = vec![-28 * (1 << 24); coded * super::CELT_NUM_BANDS];
        let lm = (self.frame_size / 120).ilog2() as u8;
        let mut signal = Vec::with_capacity(channels);
        for channel in 0..channels {
            while self.state.overlap_buffers.len() <= channel {
                self.state.overlap_buffers.push(Vec::new());
            }
            signal.push(transform::synthesize(
                &vec![0; self.frame_size],
                false,
                &mut self.state.overlap_buffers[channel],
            ));
        }
        self.postfilter.apply(&mut signal, None, lm);
        self.loss
            .good(&signal, &self.state.overlap_buffers, &energy, lm);
        self.energy_q24
            .update(energy, coded, false, self.start_band, self.end_band);
        self.state.anti_collapse_state.seed = entropy.get_range();
        DecodedFrame {
            samples: (0..self.frame_size)
                .flat_map(|i| signal.iter().map(move |c| c[i]))
                .collect(),
            sample_rate: self.sample_rate,
            channels: self.synthesis_channels,
        }
    }
}

/// Crossfade exactly 2.5 ms of interleaved non-RES24 output PCM.
///
/// Uses the pinned CELT window squared in Q15, truncating the weighted sum
/// with an arithmetic shift (no rounding). Inputs are already deemphasized,
/// rate-converted PCM, not Q12. Does not alter decoder state.
///
/// # Errors
/// * Rejects rates other than 8/12/16/24/48 kHz or input/output lengths other
///   than exactly `rate / 400 * channels`. Output is unchanged on error.
pub fn smooth_transition(
    old: &[i16],
    new: &[i16],
    output: &mut [i16],
    rate: SampleRate,
    channels: Channels,
) -> Result<()> {
    let rate = rate as usize;
    if !matches!(rate, 8000 | 12000 | 16000 | 24000 | 48000) {
        return Err(Error::CeltDecoder("invalid transition output rate".into()));
    }
    let channels = channels as usize;
    let count = rate / 400 * channels;
    if old.len() != count || new.len() != count || output.len() != count {
        return Err(Error::CeltDecoder("invalid transition PCM shape".into()));
    }
    for i in 0..count {
        let window = i32::from(transform::window(i / channels * (48000 / rate)));
        let weight = (window * window) >> 15;
        let mixed = weight * i32::from(new[i]) + (32767 - weight) * i32::from(old[i]);
        output[i] = (mixed >> 15) as i16;
    }
    Ok(())
}
