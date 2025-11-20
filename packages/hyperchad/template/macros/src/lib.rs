#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
//! Procedural macros for the `HyperChad` template system.
//!
//! This crate provides the [`container!`] macro for writing HTML-like templates with Rust syntax.
//! The macro generates `Vec<Container>` structures that can be rendered to HTML or other formats.
//!
//! # Example
//!
//! ```rust
//! use hyperchad_template_macros::container;
//!
//! let html = container! {
//!     div.container {
//!         h1 { "Hello, World!" }
//!         button hx-post="/submit" { "Click me" }
//!     }
//! };
//! ```
//!
//! See the [`container!`] macro documentation for complete syntax details and examples.

extern crate proc_macro;

mod ast;
mod generate;

use ast::DiagnosticParse;
use proc_macro2::{Ident, Span, TokenStream};
use proc_macro2_diagnostics::{Diagnostic, SpanDiagnosticExt};
use quote::quote;
use syn::parse::{ParseStream, Parser};

/// Preprocesses the token stream to handle numeric literals with CSS units.
///
/// Combines numeric tokens followed by unit identifiers or `%` into string literals,
/// enabling syntax like `100%`, `50vw`, `3.14rem` without requiring quotes.
///
/// # Examples
///
/// - `100%` becomes `"100%"`
/// - `50vw` becomes `"50vw"`
/// - `1.5em` becomes `"1.5em"`
fn preprocess_numeric_units(input: TokenStream) -> TokenStream {
    let mut output = Vec::new();
    let tokens: Vec<proc_macro2::TokenTree> = input.into_iter().collect();
    let mut i = 0;

    while i < tokens.len() {
        match &tokens[i] {
            // Look for numeric literal followed by % or unit identifier
            proc_macro2::TokenTree::Literal(lit) => {
                let lit_str = lit.to_string();

                // Check if this is a numeric literal (integer or float)
                if lit_str.parse::<f64>().is_ok() || lit_str.parse::<i64>().is_ok() {
                    // Look ahead to see if the next token is % or a unit identifier
                    if i + 1 < tokens.len() {
                        match &tokens[i + 1] {
                            // Handle percentage: 100%
                            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '%' => {
                                // Create a string literal for the combined value
                                let combined_lit =
                                    proc_macro2::Literal::string(&format!("{lit_str}%"));
                                output.push(proc_macro2::TokenTree::Literal(combined_lit));
                                i += 2; // Skip both tokens
                                continue;
                            }
                            // Handle unit identifiers: 50vw, 100vh, etc.
                            proc_macro2::TokenTree::Ident(ident) => {
                                let unit = ident.to_string();
                                match unit.as_str() {
                                    "vw" | "vh" | "dvw" | "dvh" | "px" | "em" | "rem" | "ch"
                                    | "ex" | "pt" | "pc" | "in" | "cm" | "mm" => {
                                        // Create a string literal for the combined value
                                        let combined_lit = proc_macro2::Literal::string(&format!(
                                            "{lit_str}{unit}"
                                        ));
                                        output.push(proc_macro2::TokenTree::Literal(combined_lit));
                                        i += 2; // Skip both tokens
                                        continue;
                                    }
                                    _ => {
                                        // Not a unit we recognize, process normally
                                    }
                                }
                            }
                            _ => {
                                // Not a % or unit, process normally
                            }
                        }
                    }
                }

                // Default: just add the literal as-is
                output.push(tokens[i].clone());
                i += 1;
            }
            // Handle groups (parentheses, brackets, braces) recursively
            proc_macro2::TokenTree::Group(group) => {
                let preprocessed_stream = preprocess_numeric_units(group.stream());
                let new_group = proc_macro2::Group::new(group.delimiter(), preprocessed_stream);
                output.push(proc_macro2::TokenTree::Group(new_group));
                i += 1;
            }
            // All other tokens pass through unchanged
            _ => {
                output.push(tokens[i].clone());
                i += 1;
            }
        }
    }

    output.into_iter().collect()
}

/// Procedural macro for writing HTML-like templates with Rust syntax.
///
/// This macro parses HTML-like template syntax and generates `Vec<Container>` structures
/// that can be rendered to HTML or other formats through the `HyperChad` rendering system.
///
/// # Template Syntax
///
/// The macro supports:
///
/// * HTML-like element syntax: `div { "content" }`, `button { "Click me" }`
/// * Attributes: `input type="text" name="field";`
/// * Dynamic expressions: `div { (variable) }`
/// * CSS-like selectors: `div.container #main { }`
/// * Control flow: `@if`, `@else`, `@for`, `@while`, `@match`
/// * HTMX attributes: `hx-get`, `hx-post`, `hx-trigger`, etc.
/// * Interactive behaviors: `fx-click`, `fx-hover`, etc.
///
/// # Examples
///
/// Basic usage:
///
/// ```rust
/// use hyperchad_template_macros::container;
///
/// let username = "Alice";
/// let items = vec!["Apple", "Banana", "Cherry"];
///
/// let html = container! {
///     div.container {
///         h1 { "Welcome, " (username) }
///
///         @if !items.is_empty() {
///             ul {
///                 @for item in items {
///                     li { (item) }
///                 }
///             }
///         }
///
///         input type="text" name="search" placeholder="Search...";
///
///         button hx-post="/search" hx-trigger="click" {
///             "Search"
///         }
///     }
/// };
/// ```
///
/// For comprehensive documentation on template syntax, control flow, CSS units, colors,
/// and interactive behaviors with the `fx` DSL, see the crate README.
#[proc_macro]
pub fn container(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input2 = proc_macro2::TokenStream::from(input);

    // Preprocess to handle numeric + unit combinations
    let preprocessed = preprocess_numeric_units(input2);

    match expand(preprocessed) {
        Ok(tokens) => tokens.into(),
        Err(error_msg) => quote! {
            compile_error!(#error_msg)
        }
        .into(),
    }
}

fn expand(input: TokenStream) -> Result<TokenStream, String> {
    let mut diagnostics = Vec::new();

    let markups = match Parser::parse2(
        |input: ParseStream| ast::Markups::diagnostic_parse(input, &mut diagnostics),
        input,
    ) {
        Ok(data) => data,
        Err(err) => {
            diagnostics.push(err.span().error(err.to_string()));
            // Return empty markups so diagnostics can be emitted
            ast::Markups {
                markups: Vec::new(),
            }
        }
    };

    let diag_tokens = diagnostics.into_iter().map(Diagnostic::emit_as_expr_tokens);

    let output_ident = Ident::new("__hyperchad_template_output", Span::mixed_site());
    let stmts = match generate::generate(markups, output_ident.clone()) {
        Ok(stmts) => stmts,
        Err(gen_error) => {
            return Err(format!(
                "Code generation failed in container! macro.\n\
                \n\
                Error: {gen_error}\n\
                \n\
                This usually indicates:\n\
                - Unknown element types\n\
                - Invalid attribute combinations\n\
                - Unsupported template features\n\
                - Complex nested structures that failed to generate"
            ));
        }
    };

    Ok(quote! {{
        use hyperchad_template::prelude::*;
        let mut #output_ident: Vec<hyperchad_transformer::Container> = Vec::new();
        #stmts
        #(#diag_tokens)*
        #output_ident
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test_log::test]
    fn test_preprocess_numeric_units_percentage() {
        let input = quote! { 100 % };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"100%\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_float_percentage() {
        let input = quote! { 33.3 % };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"33.3%\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_vw() {
        let input = quote! { 50 vw };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"50vw\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_vh() {
        let input = quote! { 100 vh };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"100vh\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_dvw() {
        let input = quote! { 90 dvw };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"90dvw\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_dvh() {
        let input = quote! { 75 dvh };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"75dvh\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_px() {
        let input = quote! { 16 px };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"16px\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_em() {
        let input = quote! { 1.5 em };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"1.5em\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_rem() {
        let input = quote! { 2 rem };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"2rem\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_ch() {
        let input = quote! { 10 ch };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"10ch\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_ex() {
        let input = quote! { 5 ex };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"5ex\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_pt() {
        let input = quote! { 12 pt };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"12pt\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_pc() {
        let input = quote! { 1 pc };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"1pc\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_inches() {
        let input = quote! { 2 in };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"2in\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_cm() {
        let input = quote! { 5 cm };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"5cm\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_mm() {
        let input = quote! { 10 mm };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "\"10mm\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_plain_number() {
        let input = quote! { 42 };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "42");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_plain_float() {
        let input = quote! { 3.14 };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "3.14");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_non_unit_identifier() {
        // Non-unit identifiers should be left unchanged
        let input = quote! { 10 foo };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "10 foo");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_in_braces() {
        // Test that preprocessing works recursively inside braces
        let input = quote! { { 50 vw } };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "{ \"50vw\" }");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_in_parentheses() {
        // Test that preprocessing works recursively inside parentheses
        let input = quote! { ( 100 % ) };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "(\"100%\")");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_multiple_values() {
        // Test multiple numeric+unit combinations in same stream
        let input = quote! { width = 50 vw height = 100 vh };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "width = \"50vw\" height = \"100vh\"");
    }

    #[test_log::test]
    fn test_preprocess_numeric_units_mixed_with_other_tokens() {
        // Test that non-numeric tokens are preserved
        let input = quote! { div { width = 50 % } };
        let result = preprocess_numeric_units(input);
        assert_eq!(result.to_string(), "div { width = \"50%\" }");
    }
}
