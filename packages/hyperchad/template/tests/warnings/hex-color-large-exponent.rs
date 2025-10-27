//! Test case for hex colors with very large exponents
//! 
//! Pattern like #1e999 is valid scientific notation (1e999 is a huge number)
//! and will tokenize successfully as a float literal.
//!
//! However, it results in only 5 characters: "1e999"
//! This should fail validation because hex colors must be 3, 6, or 8 chars.
//!
//! This test documents that large exponents are valid float literals but
//! produce invalid hex color lengths.

use hyperchad_template::container;

fn main() {
    // This should fail with: "Invalid hex color '#1e999'.
    // Hex colors must be 3, 6, or 8 hexadecimal digits"
    let _ = container! {
        div color=#1e999 { "5 characters - invalid length" }
    };
}
