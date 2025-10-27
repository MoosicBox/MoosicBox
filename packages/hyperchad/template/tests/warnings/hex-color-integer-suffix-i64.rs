//! Test case for integer literals with i64 suffix
//! 
//! Pattern: #abci64
//! Even though 'a', 'b', 'c' are hex-valid, the suffix "i64" contains 'i'
//! which is NOT a hex digit (hex digits are 0-9, a-f).
//!
//! Our parser extracts the base digits and suffix separately, then validates
//! that both are hex-valid. The 'i' in "i64" will cause validation to fail.

use hyperchad_template::container;

fn main() {
    // This should fail because 'i' in the suffix "i64" is not a hex digit
    let _ = container! {
        div color=#abci64 { "Hex-looking int with i64 suffix - 'i' not hex" }
    };
}
