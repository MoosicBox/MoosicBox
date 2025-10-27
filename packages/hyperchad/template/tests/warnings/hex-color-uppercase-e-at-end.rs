//! Test case for hex colors ending in uppercase 'E' without digits after
//! 
//! Similar to lowercase 'e', uppercase 'E' is also treated as scientific notation
//! by Rust's lexer. "12E" is incomplete scientific notation (needs: 12E5, 12E10, etc.)
//!
//! Workaround: Use string literal syntax instead: color="#12E"

use hyperchad_template::container;

fn main() {
    // This will fail with: "expected at least one digit in exponent"
    let _ = container! {
        div color=#12E { "Uppercase E causes same lexer error as lowercase" }
    };
}
