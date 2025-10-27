//! Test case for hex colors with negative exponent pattern
//! 
//! Pattern like #1e-2 contains a minus sign, which breaks tokenization.
//! Rust's lexer would tokenize this as multiple tokens: # + 1e + - + 2
//! The minus sign makes this invalid as a hex color.
//!
//! This is not a valid hex color pattern in any case.

use hyperchad_template::container;

fn main() {
    // This will fail - minus sign is not a valid hex digit
    let _ = container! {
        div color=#1e-2 { "Negative exponent with minus sign" }
    };
}
