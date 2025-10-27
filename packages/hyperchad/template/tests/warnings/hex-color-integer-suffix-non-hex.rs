//! Test case for integer literals with non-hex suffixes
//! 
//! Pattern: #123u32
//! Rust integer literals can have type suffixes like u32, i64, usize, etc.
//! However, these suffixes contain non-hex characters:
//! - u32: contains 'u' (not hex)
//! - i64: contains 'i' (not hex) 
//! - u8, i8, etc.: contain 'u' or 'i' (not hex)
//!
//! Our parser should reject these because the suffix is not hex-valid.

use hyperchad_template::container;

fn main() {
    // This should fail because 'u' in the suffix "u32" is not a hex digit
    let _ = container! {
        div color=#123u32 { "Integer with u32 suffix - 'u' not hex" }
    };
}
