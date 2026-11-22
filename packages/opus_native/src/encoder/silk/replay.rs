//! Forward-only SILK parameter/pulse replay allocation.
use super::{RangeEncoder, Result, entropy};
use crate::range::RangeEncoderError;

#[cfg(test)]
std::thread_local! {
    pub(super) static LONG: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static DIFFERENTIAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static CHECKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Encode forward-only symbols, retrying only exhausted storage. No raw bits
/// may be added here: raw bits are placed relative to the buffer's far end.
pub(super) fn encode(encode: impl Fn(&mut RangeEncoder) -> Result<()>) -> Result<Vec<u8>> {
    let capacity = 1275;
    #[cfg(test)]
    let capacity = if LONG.get() { 65536 } else { capacity };
    let mut coder = RangeEncoder::new(capacity).map_err(entropy)?;
    let encoded = encode(&mut coder);
    // The domain encoder maps entropy errors through `entropy`; require the
    // exact mapped capacity variant as well as the sticky capacity flag.
    let exhausted = coder.has_error()
        && matches!(&encoded,
        Err(crate::Error::Unsupported(message))
            if message == &format!("CELT encoder entropy coding: {}", RangeEncoderError::BufferTooSmall));
    let short = match encoded {
        Err(error) if !exhausted => return Err(error),
        Err(_) => None,
        Ok(()) => {
            #[cfg(test)]
            if DIFFERENTIAL.get() {
                let mut long = RangeEncoder::new(65536).map_err(entropy)?;
                encode(&mut long)?;
                assert_eq!(coder.tell(), long.tell());
                assert_eq!(coder.tell_frac(), long.tell_frac());
                assert_eq!(coder.range(), long.range());
            }
            match coder.finish() {
                Ok(bytes) => Some(bytes),
                Err(RangeEncoderError::BufferTooSmall) => None,
                Err(error) => return Err(entropy(error)),
            }
        }
    };
    if let Some(bytes) = short {
        #[cfg(test)]
        if DIFFERENTIAL.get() {
            let mut long = RangeEncoder::new(65536).map_err(entropy)?;
            encode(&mut long)?;
            let long = long.finish().map_err(entropy)?;
            assert_eq!(bytes, long[..bytes.len()]);
            assert!(long[bytes.len()..].iter().all(|&byte| byte == 0));
            CHECKED.set(CHECKED.get() + 1);
        }
        return Ok(bytes);
    }
    let mut coder = RangeEncoder::new(65536).map_err(entropy)?;
    encode(&mut coder)?;
    coder.finish().map_err(entropy)
}

#[test]
fn replay_retries_capacity_only() {
    use super::super::Error;
    let calls = std::cell::Cell::new(0);
    let bytes = encode(|coder| {
        calls.set(calls.get() + 1);
        for _ in 0..2000 {
            coder.ec_enc_uint(123, 256).map_err(entropy)?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(bytes.len(), 65536);
    calls.set(0);
    let result = encode(|_| {
        calls.set(calls.get() + 1);
        Err(Error::Unsupported("sentinel".into()))
    });
    assert!(matches!(result, Err(Error::Unsupported(message)) if message == "sentinel"));
    assert_eq!(calls.get(), 1);
}
