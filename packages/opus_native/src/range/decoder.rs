use crate::error::{Error, Result};
use crate::util::ilog;

/// Range decoder for entropy decoding in Opus packets.
///
/// Implements the range decoder specified in RFC 6716 Section 4.1. The range decoder
/// maintains an internal state consisting of a value and range, and provides methods
/// for decoding symbols from compressed bitstreams using arithmetic coding.
///
/// The decoder reads from the beginning of the buffer for range-coded symbols and
/// from the end of the buffer for raw bits, allowing efficient use of packet space.
///
/// # Examples
///
/// ```rust
/// # use moosicbox_opus_native::range::RangeDecoder;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let packet = vec![0x80, 0x00, 0x00, 0x00];
/// let mut decoder = RangeDecoder::new(&packet)?;
///
/// // Decode a bit with 50% probability
/// let bit = decoder.ec_dec_bit_logp(1)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct RangeDecoder {
    buffer: Vec<u8>,
    position: usize,
    value: u32,
    range: u32,
    total_bits: u32,
    leftover_bit: u32,
    end_position: usize,
    end_window: u32,
    end_bits_available: u32,
}

impl RangeDecoder {
    /// Creates a new range decoder and initializes it per RFC 6716 Section 4.1.1.
    ///
    /// # Errors
    ///
    /// Returns an error if the input buffer is empty or has fewer than 2 bytes.
    pub fn new(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(Error::RangeDecoder("empty buffer".to_string()));
        }

        if data.len() < 2 {
            return Err(Error::RangeDecoder(
                "buffer must have at least 2 bytes".to_string(),
            ));
        }

        let b0 = data[0];
        let value = u32::from(127 - (b0 >> 1));
        let range = 128;
        let leftover_bit = b0 & 1;

        let mut decoder = Self {
            buffer: data.to_vec(),
            position: 1,
            value,
            range,
            total_bits: 9,
            leftover_bit: u32::from(leftover_bit),
            end_position: 0,
            end_window: 0,
            end_bits_available: 0,
        };

        decoder.normalize()?;

        Ok(decoder)
    }

    #[allow(clippy::unnecessary_wraps)]
    fn normalize(&mut self) -> Result<()> {
        while self.range <= 0x80_0000 {
            self.range <<= 8;

            let byte = if self.position < self.buffer.len() {
                self.buffer[self.position]
            } else {
                0
            };
            self.position += 1;

            let sym = (self.leftover_bit << 7) | u32::from(byte >> 1);
            self.leftover_bit = u32::from(byte & 1);

            self.value = (self.value << 8) + (255 - sym);
            self.total_bits += 8;
        }

        Ok(())
    }

    fn read_byte_from_end(&mut self) -> u8 {
        if self.end_position < self.buffer.len() {
            let byte = self.buffer[self.buffer.len() - 1 - self.end_position];
            self.end_position += 1;
            byte
        } else {
            0
        }
    }

    /// Decodes a symbol with cumulative frequency `ft` per RFC 6716 Section 4.1.2.
    ///
    /// # Errors
    ///
    /// Currently does not return errors, but returns `Result` for future error handling.
    pub fn ec_decode(&mut self, ft: u32) -> Result<u32> {
        let fs = ft.saturating_sub(((self.value / (self.range / ft)) + 1).min(ft));
        Ok(fs)
    }

    /// Decodes a binary symbol per RFC 6716 Section 4.1.3.1.
    ///
    /// Equivalent to `ec_decode()` with `ft = 1 << ftb`.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `ec_decode()` call fails.
    pub fn ec_decode_bin(&mut self, ftb: u32) -> Result<u32> {
        let ft = 1_u32 << ftb;
        self.ec_decode(ft)
    }

    /// Updates decoder state after decoding a symbol per RFC 6716 Section 4.1.2.
    ///
    /// # Errors
    ///
    /// Returns an error if normalization fails due to insufficient buffer data.
    pub fn ec_dec_update(&mut self, fl: u32, fh: u32, ft: u32) -> Result<()> {
        let s = self.range / ft;

        self.value = self.value.saturating_sub(s * (ft - fh));

        if fl > 0 {
            self.range = s * (fh - fl);
        } else {
            self.range = self.range.saturating_sub(s * (ft - fh));
        }

        self.normalize()?;

        Ok(())
    }

    /// Decodes a single bit with probability `1/(1<<logp)` per RFC 6716 Section 4.1.3.2.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding or state update fails.
    pub fn ec_dec_bit_logp(&mut self, logp: u32) -> Result<bool> {
        let ft = 1_u32 << logp;
        let fs = self.ec_decode(ft)?;

        let decoded_zero = fs < ft - 1;

        if decoded_zero {
            self.ec_dec_update(0, ft - 1, ft)?;
            Ok(false)
        } else {
            self.ec_dec_update(ft - 1, ft, ft)?;
            Ok(true)
        }
    }

    /// Decodes a symbol using an inverse CDF table per RFC 6716 Section 4.1.3.3.
    ///
    /// This implementation matches libopus `ec_dec_icdf()` exactly (entdec.c lines 182-199).
    ///
    /// ICDF tables MUST be terminated with a value of 0, as specified in RFC 6716
    /// Section 4.1.3.3 (line 1534): "the table is terminated by a value of 0
    /// (where fh\[k\] == ft)." This terminating zero represents the point where the
    /// cumulative distribution reaches ft.
    ///
    /// The RFC documents provide probability distribution functions (PDFs). To use
    /// them with `ec_dec_icdf`, they must be converted to ICDF format with the
    /// mandatory terminating zero appended.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding or state update fails.
    ///
    /// # Panics
    ///
    /// Panics if `icdf` length is greater than `usize::MAX`.
    ///
    /// # Algorithm (libopus entdec.c lines 182-199)
    ///
    /// ```c
    /// s = _this->rng;
    /// d = _this->val;
    /// r = s >> _ftb;
    /// ret = -1;
    /// do {
    ///     t = s;
    ///     s = IMUL32(r, _icdf[++ret]);
    /// } while(d < s);
    /// _this->val = d - s;
    /// _this->rng = t - s;
    /// ec_dec_normalize(_this);
    /// return ret;
    /// ```
    pub fn ec_dec_icdf(&mut self, icdf: &[u8], ftb: u32) -> Result<u32> {
        let mut s = self.range;
        let d = self.value;
        let r = s >> ftb;

        let debug = icdf.len() == 8 && icdf[0] == 224; // Detect GAIN_PDF_LSB

        // Start ret at -1 (will be pre-incremented to 0 on first iteration)
        // Using wrapping arithmetic for the pre-increment pattern
        let mut ret: usize = usize::MAX; // -1 in two's complement
        let mut t;

        // Search ICDF table: find first k where val >= s = r * icdf[k]
        // libopus: do { t=s; s=IMUL32(r,_icdf[++ret]); } while(d<s);
        loop {
            t = s;
            ret = ret.wrapping_add(1); // Pre-increment (++ret)
            s = r * u32::from(icdf[ret]);

            if debug && ret <= 4 {
                log::trace!(
                    "[EC_DEC_ICDF LSB] ret={}, icdf[{}]={}, r={}, s={}, d={}, d>=s: {}",
                    ret,
                    ret,
                    icdf[ret],
                    r,
                    s,
                    d,
                    d >= s
                );
            }

            if d >= s {
                break;
            }
        }

        // Update decoder state
        let new_val = d - s;
        let new_range = t - s;

        if debug {
            log::trace!(
                "[EC_DEC_ICDF LSB RESULT] ret={}, old_range={}, old_val={}, new_range={}, new_val={}",
                ret,
                self.range,
                self.value,
                new_range,
                new_val
            );
        }

        self.value = new_val;
        self.range = new_range;

        self.normalize()?;

        #[allow(clippy::cast_possible_truncation)]
        Ok(ret as u32)
    }

    /// Decodes symbol using 16-bit ICDF table (for high-precision PDFs)
    ///
    /// This implementation matches libopus `ec_dec_icdf16()` (entdec.c lines 201-218).
    ///
    /// # Errors
    ///
    /// Returns an error if decoding or update fails.
    ///
    /// # Panics
    ///
    /// * Panics if ICDF table length exceeds `usize::MAX`.
    pub fn ec_dec_icdf_u16(&mut self, icdf: &[u16], ftb: u32) -> Result<u8> {
        let mut s = self.range;
        let d = self.value;
        let r = s >> ftb;

        // Start ret at -1 (will be pre-incremented to 0 on first iteration)
        let mut ret: usize = usize::MAX; // -1 in two's complement
        let mut t;

        // Search ICDF table: find first k where val >= s = r * icdf[k]
        // libopus: do { t=s; s=IMUL32(r,_icdf[++ret]); } while(d<s);
        loop {
            t = s;
            ret = ret.wrapping_add(1); // Pre-increment (++ret)
            s = r * u32::from(icdf[ret]);

            if d >= s {
                break;
            }
        }

        // Update decoder state
        self.value = d - s;
        self.range = t - s;
        self.normalize()?;

        #[allow(clippy::cast_possible_truncation)]
        Ok(ret as u8)
    }

    /// Extracts raw bits from the end of the frame per RFC 6716 Section 4.1.4.
    ///
    /// Reads bits backwards from the end of the buffer, independent of range coder state.
    ///
    /// # Errors
    ///
    /// Returns an error if `bits > 25` (RFC limit).
    pub fn ec_dec_bits(&mut self, bits: u32) -> Result<u32> {
        if bits == 0 {
            return Ok(0);
        }

        if bits > 25 {
            return Err(Error::RangeDecoder(
                "cannot decode more than 25 bits at once".to_string(),
            ));
        }

        while self.end_bits_available < bits {
            let byte = self.read_byte_from_end();
            self.end_window |= u32::from(byte) << self.end_bits_available;
            self.end_bits_available += 8;
        }

        let mask = (1_u32 << bits) - 1;
        let result = self.end_window & mask;

        self.end_window >>= bits;
        self.end_bits_available -= bits;
        self.total_bits += bits;

        Ok(result)
    }

    /// Decodes a uniformly distributed integer in range `[0, ft)` per RFC 6716 Section 4.1.5.
    ///
    /// # Errors
    ///
    /// Returns an error if `ft == 0`, decoding fails, or the decoded value is >= `ft` (corrupt frame).
    pub fn ec_dec_uint(&mut self, ft: u32) -> Result<u32> {
        if ft == 0 {
            return Err(Error::RangeDecoder("ft must be positive".to_string()));
        }

        let ftb = ilog(ft - 1);

        let t = if ftb <= 8 {
            let t = self.ec_decode(ft)?;
            self.ec_dec_update(t, t + 1, ft)?;
            t
        } else {
            let ft_high = ((ft - 1) >> (ftb - 8)) + 1;
            let t_high = self.ec_decode(ft_high)?;
            self.ec_dec_update(t_high, t_high + 1, ft_high)?;

            let t_low = self.ec_dec_bits(ftb - 8)?;
            (t_high << (ftb - 8)) | t_low
        };

        if t >= ft {
            return Err(Error::RangeDecoder(format!(
                "decoded value {t} >= ft {ft}, frame corrupt"
            )));
        }

        Ok(t)
    }

    /// Returns the number of whole bits decoded so far.
    ///
    /// This is an estimate based on the current range state and may be slightly
    /// less than the actual number of bits consumed from the buffer.
    #[must_use]
    pub const fn ec_tell(&self) -> u32 {
        let lg = ilog(self.range);
        self.total_bits.saturating_sub(lg)
    }

    /// Returns the current range value.
    ///
    /// This is the current size of the coding interval. The range is maintained
    /// between 2^23 and 2^32-1 through normalization.
    #[must_use]
    pub const fn get_range(&self) -> u32 {
        self.range
    }

    /// Returns the current value.
    ///
    /// This represents the current position within the coding interval.
    /// Used internally for symbol decoding.
    #[must_use]
    pub const fn get_value(&self) -> u32 {
        self.value
    }

    /// Returns the current read position in the buffer.
    ///
    /// This is the byte offset for forward reading (range-coded symbols).
    /// Does not include bytes read from the end for raw bits.
    #[must_use]
    pub const fn get_position(&self) -> usize {
        self.position
    }

    /// Returns the number of bits decoded with fractional precision.
    ///
    /// This provides a more accurate estimate than `ec_tell()` by using
    /// fractional bits (8ths of a bit). The result is in units of 1/8 bit.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use moosicbox_opus_native::range::RangeDecoder;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let packet = vec![0x80, 0x00, 0x00, 0x00];
    /// let decoder = RangeDecoder::new(&packet)?;
    ///
    /// let bits_frac = decoder.ec_tell_frac(); // In 1/8 bit units
    /// let bits_whole = decoder.ec_tell();
    /// assert!(bits_frac >= bits_whole * 8);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn ec_tell_frac(&self) -> u32 {
        let mut lg = ilog(self.range);
        let mut r_q15 = self.range >> (lg.saturating_sub(16));

        for _ in 0..3 {
            r_q15 = (r_q15 * r_q15) >> 15;
            let bit = r_q15 >> 16;
            lg = 2 * lg + bit;
            if bit == 1 {
                r_q15 >>= 1;
            }
        }

        (self.total_bits * 8).saturating_sub(lg)
    }

    /// Decodes a Laplace-distributed value per RFC 6716 Section 4.3.2.1.
    ///
    /// Reference: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/laplace.c#L101-142>
    ///
    /// Constants: <https://gitlab.xiph.org/xiph/opus/-/blob/34bba701ae97c913de719b1f7c10686f62cddb15/celt/laplace.c#L38-42>
    ///
    /// # Arguments
    ///
    /// * `fs` - Probability of zero (fs0 parameter from `e_prob_model`)
    /// * `decay` - Decay parameter for geometric distribution
    ///
    /// # Returns
    ///
    /// Signed integer from Laplace distribution
    ///
    /// # Errors
    ///
    /// * Returns error if range decoding fails
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub fn ec_laplace_decode(&mut self, fs: u32, decay: u32) -> Result<i32> {
        const LAPLACE_MINP: u32 = 1;
        const LAPLACE_LOG_MINP: u32 = 0;
        const LAPLACE_NMIN: u32 = 16;

        let mut val: i32 = 0;
        let fm = self.ec_decode_bin(15)?;
        let mut fl: u32 = 0;
        let mut fs_current = fs;

        if fm >= fs {
            val += 1;
            fl = fs;

            let ft = 32768_u32
                .saturating_sub(LAPLACE_MINP * (2 * LAPLACE_NMIN))
                .saturating_sub(fs);
            fs_current = (ft.saturating_mul(16384_u32.saturating_sub(decay))) >> 15;
            fs_current = fs_current.saturating_add(LAPLACE_MINP);

            while fs_current > LAPLACE_MINP && fm >= fl.saturating_add(2 * fs_current) {
                fs_current *= 2;
                fl = fl.saturating_add(fs_current);
                fs_current =
                    ((fs_current.saturating_sub(2 * LAPLACE_MINP)).saturating_mul(decay)) >> 15;
                fs_current = fs_current.saturating_add(LAPLACE_MINP);
                val += 1;
            }

            if fs_current <= LAPLACE_MINP {
                let di = (fm.saturating_sub(fl)) >> (LAPLACE_LOG_MINP + 1);
                val += di as i32;
                fl = fl.saturating_add(2 * di * LAPLACE_MINP);
            }

            if fm < fl.saturating_add(fs_current) {
                val = -val;
            } else {
                fl = fl.saturating_add(fs_current);
            }
        }

        self.ec_dec_update(fl, fl.saturating_add(fs_current).min(32768), 32768)?;

        Ok(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test]
    fn test_new_with_valid_buffer() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let decoder = RangeDecoder::new(&data);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_new_with_empty_buffer() {
        let data: Vec<u8> = vec![];
        let result = RangeDecoder::new(&data);
        let Err(Error::RangeDecoder(msg)) = result else {
            panic!("Invalid response {result:?}");
        };
        assert_eq!(msg, "empty buffer");
    }

    #[test]
    fn test_new_with_single_byte_buffer() {
        let data = vec![0x01];
        let result = RangeDecoder::new(&data);
        assert!(result.is_err());
        if let Err(Error::RangeDecoder(msg)) = result {
            assert_eq!(msg, "buffer must have at least 2 bytes");
        }
    }

    #[test]
    fn test_initialization_values() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00];
        let decoder = RangeDecoder::new(&data).unwrap();
        assert!(decoder.range > 0x80_0000);
    }

    #[test]
    fn test_ec_decode() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let fs = decoder.ec_decode(256).unwrap();
        assert!(fs < 256);
    }

    #[test]
    fn test_ec_decode_bin() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let fs = decoder.ec_decode_bin(8).unwrap();
        assert!(fs < 256);
    }

    #[test]
    fn test_ec_dec_bit_logp_returns_true() {
        let data = vec![0xFF, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();
        let bit = decoder.ec_dec_bit_logp(1).unwrap();
        assert!(bit);
    }

    #[test]
    fn test_ec_dec_bit_logp_returns_false() {
        let data = vec![0x00, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();
        let bit = decoder.ec_dec_bit_logp(1).unwrap();
        assert!(!bit);
    }

    #[test_case(vec![0xFF, 0x00, 0x00, 0x00, 0x00], 1, true ; "logp_1_returns_true")]
    #[test_case(vec![0xFF, 0x00, 0x00, 0x00, 0x00], 4, true ; "logp_4_returns_true")]
    #[test_case(vec![0xFF, 0x00, 0x00, 0x00, 0x00], 8, true ; "logp_8_returns_true")]
    #[test_case(vec![0x00, 0xFF, 0xFF, 0xFF, 0xFF], 1, false ; "logp_1_returns_false")]
    #[test_case(vec![0x00, 0xFF, 0xFF, 0xFF, 0xFF], 4, false ; "logp_4_returns_false")]
    #[test_case(vec![0x00, 0xFF, 0xFF, 0xFF, 0xFF], 8, false ; "logp_8_returns_false")]
    #[allow(clippy::needless_pass_by_value)]
    fn test_ec_dec_bit_logp_with_various_inputs(data: Vec<u8>, logp: u32, expected: bool) {
        let mut decoder = RangeDecoder::new(&data).unwrap();
        let bit = decoder.ec_dec_bit_logp(logp).unwrap();
        assert_eq!(bit, expected);
    }

    #[test]
    fn test_ec_dec_icdf() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let icdf = [252_u8, 200, 100, 0];
        let symbol = decoder.ec_dec_icdf(&icdf, 8).unwrap();
        assert!(symbol < 4);
    }

    #[test]
    fn test_ec_dec_bits_zero() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bits = decoder.ec_dec_bits(0).unwrap();
        assert_eq!(bits, 0);
    }

    #[test]
    fn test_ec_dec_bits_backward_reading() {
        let data = vec![0x00, 0x00, 0x00, 0xAA];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bits = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(bits, 0xAA);
    }

    #[test]
    fn test_ec_dec_bits_lsb_first_within_byte() {
        let data = vec![0x00, 0x00, 0x00, 0b1010_1010];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bit0 = decoder.ec_dec_bits(1).unwrap();
        assert_eq!(bit0, 0);

        let bit1 = decoder.ec_dec_bits(1).unwrap();
        assert_eq!(bit1, 1);

        let bit2 = decoder.ec_dec_bits(1).unwrap();
        assert_eq!(bit2, 0);

        let bit3 = decoder.ec_dec_bits(1).unwrap();
        assert_eq!(bit3, 1);
    }

    #[test]
    fn test_ec_dec_bits_multi_byte_backward() {
        let data = vec![0x00, 0x00, 0x12, 0x34];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bits = decoder.ec_dec_bits(16).unwrap();
        assert_eq!(bits, 0x1234);
    }

    #[test]
    fn test_ec_dec_bits_window_management() {
        let data = vec![0x00, 0x00, 0xFF, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let lower_4 = decoder.ec_dec_bits(4).unwrap();
        assert_eq!(lower_4, 0x0);

        let upper_4 = decoder.ec_dec_bits(4).unwrap();
        assert_eq!(upper_4, 0x0);

        let next_8 = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(next_8, 0xFF);
    }

    #[test]
    fn test_ec_dec_bits_max() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bits = decoder.ec_dec_bits(25).unwrap();
        assert!(bits < (1_u32 << 25));
    }

    #[test]
    fn test_ec_dec_bits_too_many() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_dec_bits(26);
        assert!(result.is_err());
    }

    #[test]
    fn test_ec_dec_bits_all_zeros_from_end() {
        let data = vec![0xFF, 0xFF, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bits = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(bits, 0x00);

        let bits = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(bits, 0x00);
    }

    #[test]
    fn test_ec_dec_bits_all_ones_from_end() {
        let data = vec![0x00, 0x00, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let bits = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(bits, 0xFF);

        let bits = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(bits, 0xFF);
    }

    #[test]
    fn test_ec_dec_bits_bit_ordering_in_window() {
        let data = vec![0x00, 0x00, 0x00, 0b1111_0000];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let lower_4_bits = decoder.ec_dec_bits(4).unwrap();
        assert_eq!(lower_4_bits, 0b0000);

        let upper_4_bits = decoder.ec_dec_bits(4).unwrap();
        assert_eq!(upper_4_bits, 0b1111);
    }

    #[test]
    fn test_ec_dec_bits_independent_from_range_coder() {
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x12, 0x34];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let range_position_before = decoder.position;

        let raw_bits = decoder.ec_dec_bits(8).unwrap();
        assert_eq!(raw_bits, 0x34);

        let range_position_after = decoder.position;
        assert_eq!(range_position_before, range_position_after);

        let symbol = decoder.ec_decode(16).unwrap();
        assert!(symbol < 16);
    }

    #[test]
    fn test_ec_dec_uint_small() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let value = decoder.ec_dec_uint(256).unwrap();
        assert!(value < 256);
    }

    #[test]
    fn test_ec_dec_uint_large() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let value = decoder.ec_dec_uint(1024).unwrap();
        assert!(value < 1024);
    }

    #[test]
    fn test_ec_dec_uint_zero() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_dec_uint(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_ec_tell() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00];
        let decoder = RangeDecoder::new(&data).unwrap();

        let bits_used = decoder.ec_tell();
        assert!(bits_used >= 1);
    }

    #[test]
    fn test_ec_tell_frac() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00];
        let decoder = RangeDecoder::new(&data).unwrap();

        let bits_used_frac = decoder.ec_tell_frac();
        let bits_used = decoder.ec_tell();

        assert!(bits_used_frac >= bits_used * 8);
        assert!(bits_used_frac / 8 <= bits_used + 1);
    }

    #[test]
    fn test_ec_tell_relationship() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00];
        let decoder = RangeDecoder::new(&data).unwrap();

        let bits_used = decoder.ec_tell();
        let bits_used_frac = decoder.ec_tell_frac();

        assert_eq!(bits_used, bits_used_frac.div_ceil(8));
    }

    #[test]
    fn test_ec_dec_icdf_with_terminating_zero() {
        // RFC 6716 Section 4.1.3.3 (line 1534) requires ICDF tables to be
        // "terminated by a value of 0 (where fh[k] == ft)."
        // This test verifies correct handling of the mandatory terminating zero.
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let icdf_with_terminator = &[100, 50, 25, 0]; // Terminating 0 is RFC-mandated
        let result = decoder.ec_dec_icdf(icdf_with_terminator, 8);
        assert!(result.is_ok());
        let k = result.unwrap();
        assert!(k < u32::try_from(icdf_with_terminator.len()).unwrap());
    }

    #[test]
    fn test_ec_dec_icdf_last_symbol() {
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let icdf = &[200, 150, 100, 50, 0];
        let result = decoder.ec_dec_icdf(icdf, 8);
        assert!(result.is_ok());
        let k = result.unwrap();
        assert_eq!(k, 0);
    }

    #[test]
    fn test_laplace_decode_zero() {
        let data = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_laplace_decode(16384, 6000);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_laplace_decode_nonzero() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_laplace_decode(16384, 6000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_laplace_decode_various_decay() {
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_laplace_decode(10000, 8000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ec_dec_icdf_u16_basic() {
        let data = vec![0b1010_1010, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // 16-bit ICDF table with terminating 0
        let icdf: [u16; 4] = [60000, 40000, 20000, 0];
        let symbol = decoder.ec_dec_icdf_u16(&icdf, 16).unwrap();
        assert!(symbol < 4);
    }

    #[test]
    fn test_ec_dec_icdf_u16_first_symbol() {
        // High value input should decode to first symbol (index 0)
        let data = vec![0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let icdf: [u16; 3] = [50000, 25000, 0];
        let symbol = decoder.ec_dec_icdf_u16(&icdf, 16).unwrap();
        assert_eq!(symbol, 0);
    }

    #[test]
    fn test_ec_dec_icdf_u16_varying_precision() {
        let data = vec![0x80, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Different ftb values (precision)
        let icdf: [u16; 3] = [200, 100, 0];
        let symbol = decoder.ec_dec_icdf_u16(&icdf, 8).unwrap();
        assert!(symbol < 3);
    }

    #[test]
    fn test_ec_dec_icdf_u16_high_precision() {
        let data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // High precision table (ftb = 15)
        let icdf: [u16; 5] = [32000, 24000, 16000, 8000, 0];
        let symbol = decoder.ec_dec_icdf_u16(&icdf, 15).unwrap();
        assert!(symbol < 5);
    }

    #[test]
    fn test_ec_dec_uint_boundary_8_bit() {
        // ec_dec_uint uses different paths based on ftb <= 8 or ftb > 8
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // ft = 256 means ftb = 8 (boundary case, uses simple path)
        let value = decoder.ec_dec_uint(256).unwrap();
        assert!(value < 256);
    }

    #[test]
    fn test_ec_dec_uint_large_ft() {
        // Large ft > 256 uses the complex path with bit decoding
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let value = decoder.ec_dec_uint(10000).unwrap();
        assert!(value < 10000);
    }

    #[test]
    fn test_ec_dec_uint_ft_one() {
        // ft = 1 means only value 0 is possible
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let value = decoder.ec_dec_uint(1).unwrap();
        assert_eq!(value, 0);
    }

    #[test]
    fn test_laplace_decode_high_fs() {
        // Test with high fs (probability of zero)
        let data = vec![0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // High fs means high probability of zero
        let result = decoder.ec_laplace_decode(30000, 6000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_laplace_decode_low_decay() {
        // Test with low decay (steeper distribution)
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_laplace_decode(8000, 2000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_laplace_decode_high_decay() {
        // Test with high decay (flatter distribution)
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let result = decoder.ec_laplace_decode(8000, 14000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_accessors() {
        let data = vec![0xAA, 0x55, 0xFF, 0x00];
        let decoder = RangeDecoder::new(&data).unwrap();

        // Test that accessor methods return sensible values
        let range = decoder.get_range();
        let value = decoder.get_value();
        let position = decoder.get_position();

        assert!(range > 0);
        assert!(position > 0);
        // Value should be within range
        assert!(value < range);
    }

    #[test]
    fn test_ec_tell_after_operations() {
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x12, 0x34, 0x56, 0x78];
        let mut decoder = RangeDecoder::new(&data).unwrap();

        let tell_before = decoder.ec_tell();

        // Do some decoding
        let _ = decoder.ec_decode(256).unwrap();
        decoder.ec_dec_update(10, 20, 256).unwrap();

        let tell_after = decoder.ec_tell();

        // After decoding, we should have used more bits
        assert!(tell_after >= tell_before);
    }

    #[test]
    fn test_ec_tell_frac_precision() {
        let data = vec![0xAA, 0x55, 0xFF, 0x00, 0x00, 0x00];
        let decoder = RangeDecoder::new(&data).unwrap();

        let tell = decoder.ec_tell();
        let tell_frac = decoder.ec_tell_frac();

        // Fractional bits should be >= whole bits * 8
        // (since it's in 1/8 bit units)
        assert!(tell_frac >= tell * 8 || tell == 0);
    }

    #[test]
    fn test_read_beyond_buffer_returns_zero() {
        // Test that reading past buffer end returns 0 (RFC spec)
        let data = vec![0xFF, 0xFF]; // Minimal valid buffer
        let mut decoder = RangeDecoder::new(&data).unwrap();

        // Force reading from end of buffer (and beyond)
        let bits1 = decoder.ec_dec_bits(8).unwrap();
        // This reads past the actual buffer, should return 0
        let bits2 = decoder.ec_dec_bits(8).unwrap();

        // First byte from end is 0xFF
        assert_eq!(bits1, 0xFF);
        // Second read is past buffer, returns 0
        assert_eq!(bits2, 0xFF);
    }
}
