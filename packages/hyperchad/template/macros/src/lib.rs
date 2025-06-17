#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

extern crate proc_macro;

mod ast;
mod generate;

use ast::DiagnosticParse;
use proc_macro2::{Ident, Span, TokenStream};
use proc_macro2_diagnostics::Diagnostic;
use quote::quote;
use syn::parse::{ParseStream, Parser};

/// Preprocess the token stream to handle numeric literals with units
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
            let err_str = err.to_string();

            // Provide better error messages for common parsing failures
            if err_str.contains("expected") && err_str.contains("found") {
                return Err(format!(
                    "Template syntax error in container! macro.\n\
                    \n\
                    Parsing failed: {err_str}\n\
                    \n\
                    Common issues:\n\
                    - Element names must be lowercase: div, button, h1, h2, etc.\n\
                    - Attributes: key=\"value\" or key=(expression)\n\
                    - Control flow: @if condition {{ }} @else {{ }}\n\
                    - Loops: @for item in collection {{ }}\n\
                    - Variables: @let name = value;\n\
                    - Expressions: (variable_name)\n\
                    - Input elements need semicolons: input type=\"text\";\n\
                    - Check for balanced braces {{ }}\n\
                    - Make sure all strings are properly quoted"
                ));
            }

            return Err(format!(
                "Failed to parse container! template.\n\
                \n\
                Error: {err_str}\n\
                \n\
                Common issues:\n\
                - Element names must be lowercase (div, button, etc.)\n\
                - Check attribute formatting: key=\"value\" or key=(expression)\n\
                - Ensure braces {{}} are balanced\n\
                - Control flow needs @ prefix: @if, @for, @match\n\
                - Variables need @let: @let name = value;\n\
                - Check for missing semicolons after Input elements"
            ));
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
