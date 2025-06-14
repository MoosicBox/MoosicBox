#![doc(html_root_url = "https://docs.rs/hyperchad_template_macros/0.27.0")]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::needless_pass_by_value)]

extern crate proc_macro;

mod ast;
mod generate;

use ast::DiagnosticParse;
use proc_macro2::{Ident, Span, TokenStream};
use proc_macro2_diagnostics::Diagnostic;
use quote::quote;
use syn::parse::{ParseStream, Parser};

#[proc_macro]
pub fn container(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input2 = proc_macro2::TokenStream::from(input);

    match expand(input2) {
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
                    Parsing failed: {}\n\
                    \n\
                    Common issues:\n\
                    - Element names must be PascalCase: Div, Button, H1, H2, etc.\n\
                    - Attributes: key=\"value\" or key=(expression)\n\
                    - Control flow: @if condition {{ }} @else {{ }}\n\
                    - Loops: @for item in collection {{ }}\n\
                    - Variables: @let name = value;\n\
                    - Expressions: (variable_name)\n\
                    - Input elements need semicolons: Input type=\"text\";\n\
                    - Check for balanced braces {{ }}\n\
                    - Make sure all strings are properly quoted",
                    err_str
                ));
            } else {
                return Err(format!(
                    "Failed to parse container! template.\n\
                    \n\
                    Error: {}\n\
                    \n\
                    Common issues:\n\
                    - Element names must be PascalCase (Div, Button, etc.)\n\
                    - Check attribute formatting: key=\"value\" or key=(expression)\n\
                    - Ensure braces {{}} are balanced\n\
                    - Control flow needs @ prefix: @if, @for, @match\n\
                    - Variables need @let: @let name = value;\n\
                    - Check for missing semicolons after Input elements",
                    err_str
                ));
            }
        }
    };

    let diag_tokens = diagnostics.into_iter().map(Diagnostic::emit_as_expr_tokens);

    let output_ident = Ident::new("__hyperchad_template2_output", Span::mixed_site());
    let stmts = match generate::generate(markups, output_ident.clone()) {
        Ok(stmts) => stmts,
        Err(gen_error) => {
            return Err(format!(
                "Code generation failed in container! macro.\n\
                \n\
                Error: {}\n\
                \n\
                This usually indicates:\n\
                - Unknown element types\n\
                - Invalid attribute combinations\n\
                - Unsupported template features\n\
                - Complex nested structures that failed to generate",
                gen_error
            ));
        }
    };

    Ok(quote! {{
        extern crate hyperchad_transformer;
        extern crate hyperchad_transformer_models;
        extern crate hyperchad_color;
        let mut #output_ident: Vec<hyperchad_transformer::Container> = Vec::new();
        #stmts
        #(#diag_tokens)*
        #output_ident
    }})
}
