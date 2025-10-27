//! Tests for hex color parsing edge cases, particularly scientific notation patterns
//!
//! These tests verify that hex colors containing the letter 'e' (which can be interpreted
//! as scientific notation by Rust's tokenizer) are correctly parsed.
//!
//! ## Scientific Notation Challenges
//!
//! Rust's lexer treats patterns with 'e' as scientific notation:
//! - `#1e2` → tokenized as `#` + `1e2` (float literal = 100.0) ✅ WE HANDLE THIS
//! - `#1e293b` → tokenized as `#` + `1e293b` (float with suffix "b") ✅ WE HANDLE THIS
//! - `#12e` → LEXER ERROR: "expected at least one digit in exponent" ❌ USE STRING SYNTAX
//!
//! ## Workaround for #XXe Pattern
//!
//! For hex colors ending in 'e' without digits after (like #12e, #abe), you must use
//! string literal syntax:
//! ```rust,ignore
//! div color="#12e" { }  // ✅ Works
//! div color=#12e { }    // ❌ Lexer error
//! ```
//!
//! This limitation exists because Rust's lexer processes tokens before our macro sees them,
//! and `12e` is interpreted as incomplete scientific notation.

use hyperchad_color::Color;
use hyperchad_template::container;

#[test]
fn scientific_notation_3_digit() {
    // Colors like #1e2, #3e8 are tokenized as float literals (1e2 = 100.0, 3e8 = 300000000.0)
    // but should be parsed as hex colors #11ee22, #33ee88
    let result = container! {
        div color=#1e2 { "test" }
    };

    assert_eq!(result[0].color, Some(Color::from_hex("#1e2")));
}

#[test]
fn scientific_notation_6_digit() {
    // Colors like #1e293b are tokenized as: float literal 1e293 with suffix "b"
    // This is because Rust sees "1e293b" as a float with a type suffix
    let result = container! {
        div color=#1e293b { "slate-800" }
    };

    assert_eq!(result[0].color, Some(Color::from_hex("#1e293b")));
}

#[test]
fn scientific_notation_8_digit_with_alpha() {
    // 8-digit colors with 'e' and suffixes
    let result = container! {
        div color=#1e293bff { "slate-800 with full alpha" }
    };

    assert_eq!(result[0].color, Some(Color::from_hex("#1e293bff")));
}

#[test]
fn mixed_scientific_and_normal() {
    // Test that we can mix scientific notation patterns with normal hex colors
    let result = container! {
        div {
            div color=#1e2 { "scientific 3-digit" }
            div color=#abc { "normal 3-digit" }
            div color=#1e293b { "scientific 6-digit" }
            div color=#123456 { "normal 6-digit" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#1e2")));
    assert_eq!(result[0].children[1].color, Some(Color::from_hex("#abc")));
    assert_eq!(
        result[0].children[2].color,
        Some(Color::from_hex("#1e293b"))
    );
    assert_eq!(
        result[0].children[3].color,
        Some(Color::from_hex("#123456"))
    );
}

#[test]
fn all_e_positions() {
    // Test 'e' in various positions
    let result = container! {
        div {
            div color=#e12 { "e at start" }
            div color=#1e2 { "e in middle" }
            div color=#abe { "e at end (as ident)" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#e12")));
    assert_eq!(result[0].children[1].color, Some(Color::from_hex("#1e2")));
    assert_eq!(result[0].children[2].color, Some(Color::from_hex("#abe")));
}

#[test]
fn workaround_for_xxe_pattern() {
    // Hex colors ending in 'e' without digits after (like #12e) cannot use bare syntax
    // because Rust's lexer treats "12e" as incomplete scientific notation.
    //
    // The workaround is to use string literal syntax:
    let result = container! {
        div {
            div color="#12e" { "String syntax works for #12e" }
            div color="#abe" { "String syntax works for #abe" }
            div color="#5fe" { "String syntax works for #5fe" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#12e")));
    assert_eq!(result[0].children[1].color, Some(Color::from_hex("#abe")));
    assert_eq!(result[0].children[2].color, Some(Color::from_hex("#5fe")));
}

#[test]
fn patterns_that_work_bare() {
    // These patterns work fine with bare syntax (no quotes needed)
    let result = container! {
        div {
            div color=#12f { "#12f - f is not treated as exponent" }
            div color=#e12 { "#e12 - e at start is just a hex digit" }
            div color=#1e2 { "#1e2 - valid scientific notation (1e2 = 100)" }
            div color=#2e5a3d { "#2e5a3d - scientific notation with hex suffix" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#12f")));
    assert_eq!(result[0].children[1].color, Some(Color::from_hex("#e12")));
    assert_eq!(result[0].children[2].color, Some(Color::from_hex("#1e2")));
    assert_eq!(
        result[0].children[3].color,
        Some(Color::from_hex("#2e5a3d"))
    );
}

#[test]
fn multiple_es_in_hex_color() {
    // Test colors with multiple 'e's - these should parse as identifiers
    // since they don't match scientific notation pattern (can't start with 'e')
    let result = container! {
        div {
            div color=#eee { "3 e's - parses as identifier" }
            div color=#eeeeee { "6 e's - parses as identifier" }
            div color=#e1e2e3 { "e's mixed with digits - parses as identifier" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#eee")));
    assert_eq!(
        result[0].children[1].color,
        Some(Color::from_hex("#eeeeee"))
    );
    assert_eq!(
        result[0].children[2].color,
        Some(Color::from_hex("#e1e2e3"))
    );
}

#[test]
fn uppercase_e_scientific_notation() {
    // Rust supports both lowercase 'e' and uppercase 'E' for scientific notation
    // Test that uppercase 'E' patterns work correctly
    let result = container! {
        div {
            div color=#1E2 { "Uppercase E - tokenizes as LitFloat(1E2)" }
            div color=#3E8 { "Uppercase E - tokenizes as LitFloat(3E8)" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#1E2")));
    assert_eq!(result[0].children[1].color, Some(Color::from_hex("#3E8")));
}

#[test]
fn uppercase_e_with_suffix() {
    // Test uppercase 'E' with hex-valid suffix
    let result = container! {
        div color=#1E293B { "Uppercase E with suffix - LitFloat(1E293, suffix='B')" }
    };

    assert_eq!(result[0].color, Some(Color::from_hex("#1E293B")));
}

#[test]
fn large_valid_exponents() {
    // Test scientific notation with large but valid exponents
    // These should parse correctly as long as they result in valid hex lengths
    let result = container! {
        div {
            div color=#1e9 { "1e9 = 1 billion, 3 chars" }
            div color=#2e9abc { "2e9 with suffix, 6 chars total" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#1e9")));
    assert_eq!(
        result[0].children[1].color,
        Some(Color::from_hex("#2e9abc"))
    );
}

#[test]
fn zero_exponent() {
    // Test scientific notation with zero exponent (1e0 = 1.0)
    let result = container! {
        div {
            div color=#1e0 { "1e0 = 1.0" }
            div color=#2e0 { "2e0 = 2.0" }
            div color=#9e0abc { "9e0 with suffix" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#1e0")));
    assert_eq!(result[0].children[1].color, Some(Color::from_hex("#2e0")));
    assert_eq!(
        result[0].children[2].color,
        Some(Color::from_hex("#9e0abc"))
    );
}

#[test]
fn hex_starting_with_letter_then_e_pattern() {
    // Test colors that start with a letter and contain 'eX' pattern
    // These parse as identifiers (not floats) since they don't start with a digit
    let result = container! {
        div {
            div color=#ae2 { "Starts with 'a', contains 'e2'" }
            div color=#be5abc { "Starts with 'b', contains 'e5'" }
            div color=#ce1de2 { "Multiple e-patterns, starts with letter" }
        }
    };

    assert_eq!(result[0].children[0].color, Some(Color::from_hex("#ae2")));
    assert_eq!(
        result[0].children[1].color,
        Some(Color::from_hex("#be5abc"))
    );
    assert_eq!(
        result[0].children[2].color,
        Some(Color::from_hex("#ce1de2"))
    );
}

#[test]
fn float_suffix_hex_valid() {
    // Rust float literals can have f32/f64 suffixes
    // Pattern #1e2f32: LitFloat(1e2, suffix="f32")
    // All chars are hex-valid: 1,e,2,f,3,2 = 6 chars total
    let result = container! {
        div {
            div color=#1e2f32 { "Float suffix f32 - all hex chars" }
            div color=#3e4f64 { "Float suffix f64 - all hex chars (well, '6' and '4')" }
        }
    };

    assert_eq!(
        result[0].children[0].color,
        Some(Color::from_hex("#1e2f32"))
    );
    assert_eq!(
        result[0].children[1].color,
        Some(Color::from_hex("#3e4f64"))
    );
}

#[test]
fn numeric_literal_with_hex_valid_suffix() {
    // Pattern: #555f32
    // Even though this looks like an integer literal with float suffix,
    // all characters (5,5,5,f,3,2) are hex-valid and total 6 chars
    // So it's accepted as a valid hex color!
    //
    // This demonstrates that some Rust numeric literal patterns with suffixes
    // happen to coincidentally form valid hex colors when all chars are hex-valid.
    let result = container! {
        div {
            div color=#555f32 { "Looks like int with f32 suffix, but valid hex" }
            div color=#123f64 { "123f64 - all hex chars, 6 total" }
        }
    };

    assert_eq!(
        result[0].children[0].color,
        Some(Color::from_hex("#555f32"))
    );
    assert_eq!(
        result[0].children[1].color,
        Some(Color::from_hex("#123f64"))
    );
}
