//! Hybrid CELT highband entry; packet orchestration stays with the Opus decoder.
use super::{CeltDecoder, DecodedFrame, RangeDecoder, Result};
use crate::Error;

impl CeltDecoder {
    /// Decodes the hybrid highband using the entropy decoder already advanced by SILK.
    ///
    /// `frame_bytes` is the complete entropy payload length (excluding the TOC and
    /// any trailing redundancy), not the number of unread bytes. The caller must
    /// first consume the hybrid redundancy signalling and shrink the raw-bit
    /// storage when redundancy is present, as in `opus_decoder.c`.
    ///
    /// Uses bands 17..`end_band` and persistent 48 kHz CELT predictor, TDAC and
    /// postfilter state. Configure coded channels with `set_channels` and output
    /// channels with `configure_synthesis_channels` before this call. Configure the
    /// spectral output rate with `set_output_rate`; this call preserves its
    /// pre-IMDCT frequency cutoff while always synthesizing `frame_samples` at
    /// the internal 48 kHz clock. The returned
    /// interleaved Q12 signal has **not** undergone deemphasis or SILK accumulation.
    /// The caller must retain common deemphasis history and use the reference
    /// accumulation order: deemphasize CELT, convert to the configured reference
    /// `opus_res` precision, then add the already-resampled SILK PCM. Do not add
    /// SILK to this Q12 signal before deemphasis.
    ///
    /// # Errors
    /// * Returns an error for non-hybrid durations (other than 480 or 960 samples),
    ///   invalid highband end (other than 19 or 21), empty/oversized payloads, or an
    ///   inconsistent consumed entropy budget.
    /// * Propagates entropy, allocation, reconstruction and synthesis errors.
    pub fn decode_hybrid_highband(
        &mut self,
        range_decoder: &mut RangeDecoder,
        frame_bytes: usize,
        frame_samples: usize,
        end_band: usize,
    ) -> Result<DecodedFrame> {
        if !matches!(frame_samples, 480 | 960)
            || !matches!(end_band, 19 | 21)
            || !(1..=1275).contains(&frame_bytes)
            || range_decoder.ec_tell() as usize > frame_bytes * 8
        {
            return Err(Error::CeltDecoder(
                "invalid hybrid highband configuration or entropy budget".into(),
            ));
        }
        self.set_frame_size(frame_samples)?;
        self.start_band = 17;
        self.end_band = end_band;
        self.decode_celt_frame(range_decoder, frame_bytes)
    }
}
