//! Canonical text and the four accepted UUID spellings.
use std::fmt;

pub fn display(bytes: &[u8; 16], f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, byte) in bytes.iter().enumerate() {
        if matches!(i, 4 | 6 | 8 | 10) {
            f.write_str("-")?;
        }
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

const fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub fn parse(input: &str) -> Option<[u8; 16]> {
    let bytes = input.as_bytes();
    let digits = match (bytes.len(), bytes) {
        (32 | 36, raw) | (38, [b'{', raw @ .., b'}']) => raw,
        (45, raw) if raw[..9].eq_ignore_ascii_case(b"urn:uuid:") => &raw[9..],
        _ => return None,
    };
    let mut id = [0; 16];
    let mut chars = digits.iter().copied();
    for (i, out) in id.iter_mut().enumerate() {
        if digits.len() == 36 && matches!(i, 4 | 6 | 8 | 10) && chars.next()? != b'-' {
            return None;
        }
        *out = (nibble(chars.next()?)? << 4) | nibble(chars.next()?)?;
    }
    Some(id)
}
