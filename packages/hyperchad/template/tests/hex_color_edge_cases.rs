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
