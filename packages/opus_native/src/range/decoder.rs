use crate::error::{Error, Result};
use crate::util::ilog;

#[derive(Debug)]
pub struct RangeDecoder {
    buffer: Vec<u8>,
    position: usize,
    value: u32,
    range: u32,
    total_bits: u32,
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

        let mut decoder = Self {
            buffer: data.to_vec(),
            position: 1,
            value,
            range,
            total_bits: 9,
            end_position: 0,
            end_window: 0,
            end_bits_available: 0,
        };

        decoder.normalize()?;

        Ok(decoder)
    }

    fn normalize(&mut self) -> Result<()> {
        while self.range <= 0x80_0000 {
            if self.position >= self.buffer.len() {
                return Err(Error::RangeDecoder(
                    "unexpected end of buffer during normalization".to_string(),
                ));
            }

            self.value = (self.value << 8) | u32::from(self.buffer[self.position]);
            self.range <<= 8;
            self.position += 1;
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
    /// ICDF tables MUST be terminated with a value of 0, as specified in RFC 6716
    /// Section 4.1.3.3 (line 1534): "the table is terminated by a value of 0
    /// (where fh[k] == ft)." This terminating zero represents the point where the
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
    /// Panics if `icdf` length is greater than `u32::MAX`.
    pub fn ec_dec_icdf(&mut self, icdf: &[u8], ftb: u32) -> Result<u32> {
        let ft = 1_u32 << ftb;
        let fs = self.ec_decode(ft)?;
        let len = u32::try_from(icdf.len()).expect("icdf length must fin in u32");

        let mut k: u32 = 0;
        while k < len && fs >= ft - u32::from(icdf[k as usize]) {
            k += 1;
        }

        let fl = if k == 0 {
            0
        } else {
            ft - u32::from(icdf[(k - 1) as usize])
        };
        let fh = if k >= len {
            0
        } else {
            ft - u32::from(icdf[k as usize])
        };

        self.ec_dec_update(fl, fh, ft)?;

        Ok(k)
    }

    /// Decodes symbol using 16-bit ICDF table (for high-precision PDFs)
    ///
    /// # Errors
    ///
    /// Returns an error if decoding or update fails.
    ///
    /// # Panics
    ///
    /// * Panics if ICDF table length exceeds `u32::MAX` (unrealistic for valid Opus frames).
    pub fn ec_dec_icdf_u16(&mut self, icdf: &[u16], ftb: u32) -> Result<u8> {
        let ft = 1u32 << ftb;
        let fs = self.ec_decode(ft)?;
        let len = u32::try_from(icdf.len()).expect("icdf length must fit in u32");

        let mut k: u32 = 0;
        while k < len && fs >= ft - u32::from(icdf[k as usize]) {
            k += 1;
        }

        let fl = if k == 0 {
            0
        } else {
            ft - u32::from(icdf[(k - 1) as usize])
        };
        let fh = if k >= len {
            0
        } else {
            ft - u32::from(icdf[k as usize])
        };

        self.ec_dec_update(fl, fh, ft)?;

        #[allow(clippy::cast_possible_truncation)]
        Ok(k as u8)
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

    #[must_use]
    pub const fn ec_tell(&self) -> u32 {
        let lg = ilog(self.range);
        self.total_bits.saturating_sub(lg)
    }

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
}
