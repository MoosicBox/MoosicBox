//! Integer Opus entropy encoder, translated from libopus `celt/entenc.c` at
//! revision `34bba701ae97c913de719b1f7c10686f62cddb15`.
//!
//! Arithmetic uses the reference's 32-bit unsigned range/value and carry rules.
//! Raw bits grow backwards from the packet end; do not truncate finalized output.
//! Initial-bit patching and post-initialization buffer shrinking are not exposed.
//!
//! Copyright (c) 2001-2011 Timothy B. Terriberry
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are met:
//! * Redistributions of source code must retain the above copyright notice,
//!   this list of conditions and the following disclaimer.
//! * Redistributions in binary form must reproduce the above copyright notice,
//!   this list of conditions and the following disclaimer in the documentation
//!   and/or other materials provided with the distribution.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
//! AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
//! IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
//! ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
//! LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
//! CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
//! SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
//! INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
//! CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
//! ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
//! POSSIBILITY OF SUCH DAMAGE.

const TOP: u32 = 1 << 31;
const BOT: u32 = TOP >> 8;

/// Entropy encoder failures. Invalid inputs leave the coder unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RangeEncoderError {
    /// Invalid interval, probability, symbol, bit count, or packet size.
    #[error("invalid range encoder argument")]
    InvalidArgument,
    /// Front/back output collided. The packet must not be used.
    #[error("range encoder packet budget exhausted")]
    BufferTooSmall,
}

type Result<T> = std::result::Result<T, RangeEncoderError>;

/// Fixed-budget native integer Opus entropy encoder.
///
/// Buffer exhaustion is sticky. Operations still update the arithmetic state,
/// as in libopus, but return an error and finalization refuses the packet.
#[derive(Clone, Debug)]
pub struct RangeEncoder {
    buffer: Vec<u8>,
    offs: usize,
    end_offs: usize,
    end_window: u32,
    nend_bits: u32,
    nbits_total: u64,
    rng: u32,
    val: u32,
    rem: Option<u32>,
    ext: usize,
    error: bool,
}

impl RangeEncoder {
    /// Allocate a fixed-size packet. Empty packets are permitted but cannot
    /// contain coded information. Allocation failure follows Rust's allocator.
    ///
    /// # Errors
    /// * `InvalidArgument` if the size exceeds the reference's signed bit budget.
    pub fn new(bytes: usize) -> Result<Self> {
        if bytes > i32::MAX as usize / 8 {
            return Err(RangeEncoderError::InvalidArgument);
        }
        Ok(Self {
            buffer: vec![0; bytes],
            offs: 0,
            end_offs: 0,
            end_window: 0,
            nend_bits: 0,
            nbits_total: 33,
            rng: TOP,
            val: 0,
            rem: None,
            ext: 0,
            error: false,
        })
    }

    const fn status(&self) -> Result<()> {
        if self.error {
            Err(RangeEncoderError::BufferTooSmall)
        } else {
            Ok(())
        }
    }

    fn write_byte(&mut self, value: u32, at_end: bool) {
        if self.offs + self.end_offs >= self.buffer.len() {
            self.error = true;
        } else if at_end {
            self.end_offs += 1;
            let index = self.buffer.len() - self.end_offs;
            self.buffer[index] = value.to_le_bytes()[0];
        } else {
            self.buffer[self.offs] = value.to_le_bytes()[0];
            self.offs += 1;
        }
    }

    fn carry_out(&mut self, value: u32) {
        if value == 255 {
            self.ext += 1;
        } else {
            let carry = value >> 8;
            if let Some(rem) = self.rem {
                self.write_byte(rem + carry, false);
            }
            let symbol = (255 + carry) & 255;
            while self.ext > 0 {
                self.write_byte(symbol, false);
                self.ext -= 1;
            }
            self.rem = Some(value & 255);
        }
    }

    fn normalize(&mut self) {
        while self.rng <= BOT {
            self.carry_out(self.val >> 23);
            self.val = (self.val << 8) & (TOP - 1);
            self.rng <<= 8;
            self.nbits_total += 8;
        }
    }

    fn interval(&mut self, low: u32, high: u32, total: u32, unit: u32) {
        if low > 0 {
            self.val = self.val.wrapping_add(self.rng - unit * (total - low));
            self.rng = unit * (high - low);
        } else {
            self.rng -= unit * (total - high);
        }
        self.normalize();
    }

    /// Encode cumulative interval `[low, high)` within `total`.
    ///
    /// # Errors
    /// * `InvalidArgument` unless `low < high <= total <= rng` and `total > 0`.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_encode(&mut self, low: u32, high: u32, total: u32) -> Result<()> {
        if low >= high || high > total || total > self.rng {
            return Err(RangeEncoderError::InvalidArgument);
        }
        self.interval(low, high, total, self.rng / total);
        self.status()
    }

    /// Encode a cumulative interval with total `2^bits`.
    ///
    /// # Errors
    /// * `InvalidArgument` for bits outside 1..=15 or an invalid interval.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_encode_bin(&mut self, low: u32, high: u32, bits: u32) -> Result<()> {
        if !(1..=15).contains(&bits) || low >= high || high > 1 << bits {
            return Err(RangeEncoderError::InvalidArgument);
        }
        self.interval(low, high, 1 << bits, self.rng >> bits);
        self.status()
    }

    /// Encode a bit whose probability of being true is `1 / 2^logp`.
    ///
    /// # Errors
    /// * `InvalidArgument` for logp outside 1..=15.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_enc_bit_logp(&mut self, value: bool, logp: u32) -> Result<()> {
        if !(1..=15).contains(&logp) {
            return Err(RangeEncoderError::InvalidArgument);
        }
        let small = self.rng >> logp;
        self.rng -= small;
        if value {
            self.val = self.val.wrapping_add(self.rng);
            self.rng = small;
        }
        self.normalize();
        self.status()
    }

    fn icdf(
        &mut self,
        symbol: usize,
        table: impl Iterator<Item = u32> + Clone,
        bits: u32,
    ) -> Result<()> {
        if !(1..=15).contains(&bits) {
            return Err(RangeEncoderError::InvalidArgument);
        }
        let mut previous = 1 << bits;
        let mut low = 0;
        let mut high = 0;
        let mut found = false;
        for (i, value) in table.enumerate() {
            if value >= previous {
                return Err(RangeEncoderError::InvalidArgument);
            }
            if i == symbol {
                low = (1 << bits) - previous;
                high = (1 << bits) - value;
                found = true;
            }
            previous = value;
        }
        if previous != 0 || !found {
            return Err(RangeEncoderError::InvalidArgument);
        }
        self.ec_encode_bin(low, high, bits)
    }

    /// Encode an 8-bit inverse cumulative distribution symbol.
    ///
    /// # Errors
    /// * `InvalidArgument` unless bits is 1..=8, the table strictly decreases to
    ///   zero with all entries below `2^bits`, and the symbol is in bounds.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_enc_icdf(&mut self, symbol: usize, table: &[u8], bits: u32) -> Result<()> {
        if bits > 8 {
            return Err(RangeEncoderError::InvalidArgument);
        }
        self.icdf(symbol, table.iter().copied().map(u32::from), bits)
    }

    /// Encode a 16-bit inverse cumulative distribution symbol.
    ///
    /// # Errors
    /// * `InvalidArgument` unless bits is 1..=15 and the symbol/table satisfy the
    ///   same strictly decreasing, zero-terminated constraints as 8-bit ICDF.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_enc_icdf_u16(&mut self, symbol: usize, table: &[u16], bits: u32) -> Result<()> {
        self.icdf(symbol, table.iter().copied().map(u32::from), bits)
    }

    /// Encode `value` uniformly in `0..total`, splitting large totals into range
    /// coded high bits and raw low bits exactly as libopus does.
    ///
    /// # Errors
    /// * `InvalidArgument` unless `total >= 2` and `value < total`.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_enc_uint(&mut self, value: u32, total: u32) -> Result<()> {
        if total < 2 || value >= total {
            return Err(RangeEncoderError::InvalidArgument);
        }
        let max = total - 1;
        let bits = 32 - max.leading_zeros();
        if bits > 8 {
            let raw = bits - 8;
            let upper = value >> raw;
            // Continue raw state even after a range output collision, matching C.
            let _ = self.ec_encode(upper, upper + 1, (max >> raw) + 1);
            self.ec_enc_bits(value & ((1 << raw) - 1), raw)
        } else {
            self.ec_encode(value, value + 1, total)
        }
    }

    /// Append 1..=25 raw bits to the packet's backwards-growing tail.
    ///
    /// # Errors
    /// * `InvalidArgument` for invalid width or a value not fitting that width.
    /// * `BufferTooSmall` if the packet budget has been exhausted.
    pub fn ec_enc_bits(&mut self, value: u32, bits: u32) -> Result<()> {
        if !(1..=25).contains(&bits) || value >= 1 << bits {
            return Err(RangeEncoderError::InvalidArgument);
        }
        if self.nend_bits + bits > 32 {
            loop {
                self.write_byte(self.end_window & 255, true);
                self.end_window >>= 8;
                self.nend_bits -= 8;
                if self.nend_bits < 8 {
                    break;
                }
            }
        }
        self.end_window |= value << self.nend_bits;
        self.nend_bits += bits;
        self.nbits_total += u64::from(bits);
        self.status()
    }

    /// Whole-bit ceiling of the arithmetic coder's bit usage, including the
    /// one-bit initial termination reservation (not a finalized byte count).
    #[must_use]
    pub fn tell(&self) -> u64 {
        self.nbits_total - u64::from(32 - self.rng.leading_zeros())
    }

    /// Conservative eighth-bit usage, bit-exact with reference `ec_tell_frac`.
    #[must_use]
    pub fn tell_frac(&self) -> u64 {
        const CORRECTION: [u32; 8] = [35733, 38967, 42495, 46340, 50535, 55109, 60097, 65535];
        let log = 32 - self.rng.leading_zeros();
        let range = self.rng >> (log - 16);
        let mut fraction = (range >> 12) - 8;
        fraction += u32::from(range > CORRECTION[fraction as usize]);
        (self.nbits_total << 3) - u64::from((log << 3) + fraction)
    }

    /// Configured packet budget in bytes.
    #[must_use]
    pub const fn storage(&self) -> usize {
        self.buffer.len()
    }

    /// Whether any byte write has exceeded the packet budget.
    #[must_use]
    pub const fn has_error(&self) -> bool {
        self.error
    }

    /// Current range state, for codec final-range diagnostics.
    #[must_use]
    pub const fn range(&self) -> u32 {
        self.rng
    }

    /// Terminate the arithmetic stream, flush raw bits, and return the entire
    /// fixed-budget packet. Raw bits may share the last arithmetic byte.
    ///
    /// # Errors
    /// * `BufferTooSmall` if front/back writes or final shared bits collide.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        let mut bits = self.rng.leading_zeros();
        let mut mask = (TOP - 1) >> bits;
        let mut end = self.val.wrapping_add(mask) & !mask;
        if (end | mask) >= self.val.wrapping_add(self.rng) {
            bits += 1;
            mask >>= 1;
            end = self.val.wrapping_add(mask) & !mask;
        }
        let mut remaining = i64::from(bits);
        while remaining > 0 {
            self.carry_out(end >> 23);
            end = (end << 8) & (TOP - 1);
            remaining -= 8;
        }
        if self.rem.is_some() || self.ext > 0 {
            self.carry_out(0);
        }
        let mut window = self.end_window;
        let mut used = self.nend_bits;
        while used >= 8 {
            self.write_byte(window & 255, true);
            window >>= 8;
            used -= 8;
        }
        if !self.error {
            let end_offset = self.buffer.len() - self.end_offs;
            self.buffer[self.offs..end_offset].fill(0);
            if used > 0 {
                if self.end_offs >= self.buffer.len() {
                    self.error = true;
                } else {
                    // The loop above ends with remaining in -7..=0.
                    let available = (-remaining).unsigned_abs();
                    if self.offs + self.end_offs >= self.buffer.len() && available < u64::from(used)
                    {
                        window &= (1 << available) - 1;
                        self.error = true;
                    }
                    self.buffer[end_offset - 1] |= window.to_le_bytes()[0];
                }
            }
        }
        self.status()?;
        Ok(self.buffer)
    }
}
