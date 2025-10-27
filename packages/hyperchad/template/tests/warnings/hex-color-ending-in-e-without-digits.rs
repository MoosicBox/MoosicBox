//! Test case for hex colors ending in 'e' without digits after
//!
//! This pattern fails at Rust's lexer level because "12e" is interpreted
//! as incomplete scientific notation (expected format: 12e5, 12e10, etc.)
//!
//! Workaround: Use string literal syntax instead: color="#12e"

use hyperchad_template::container;

fn main() {
    // This will fail with: "expected at least one digit in exponent"
    let _ = container! {
        div color=#12e { "This should cause a lexer error" }
    };
}
