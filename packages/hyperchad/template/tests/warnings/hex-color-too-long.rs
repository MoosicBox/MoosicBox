//! Test case for hex colors that are too long (more than 8 characters)
//! 
//! Valid hex colors must be exactly 3, 6, or 8 hexadecimal digits.
//! This test tries a 9-character hex color which should be rejected.
//!
//! Pattern: #1e2abcdef
//! Tokenizes as: # + LitFloat(1e2, suffix="abcdef")
//! Total length: "1e2" (3) + "abcdef" (6) = 9 characters (invalid)

use hyperchad_template::container;

fn main() {
    // This should fail with: "Invalid hex color '#1e2abcdef'. 
    // Hex colors must be 3, 6, or 8 hexadecimal digits"
    let _ = container! {
        div color=#1e2abcdef { "9 characters - too long" }
    };
}
