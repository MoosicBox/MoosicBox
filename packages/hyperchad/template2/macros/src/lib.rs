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
    expand(input.into()).into()
}

fn expand(input: TokenStream) -> TokenStream {
    // Heuristic: the size of the resulting markup tends to correlate with the
    // code size of the template itself
    let size_hint = input.to_string().len();

    let mut diagnostics = Vec::new();
    let markups = match Parser::parse2(
        |input: ParseStream| ast::Markups::diagnostic_parse(input, &mut diagnostics),
        input,
    ) {
        Ok(data) => data,
        Err(err) => {
            let err = err.to_compile_error();
            let diag_tokens = diagnostics.into_iter().map(Diagnostic::emit_as_expr_tokens);

            return quote! {{
                #err
                #(#diag_tokens)*
            }};
        }
    };

    let diag_tokens = diagnostics.into_iter().map(Diagnostic::emit_as_expr_tokens);

    let output_ident = Ident::new("__hyperchad_template_output", Span::mixed_site());
    let stmts = generate::generate(markups, output_ident.clone());
    quote! {{
        extern crate alloc;
        extern crate hyperchad_template2;
        let mut #output_ident = alloc::string::String::with_capacity(#size_hint);
        #stmts
        #(#diag_tokens)*
        hyperchad_template2::PreEscaped(#output_ident)
    }}
}
