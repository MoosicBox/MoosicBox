#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

extern crate proc_macro;

mod evaluator;
mod parser;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::Parser;

/// Main proc-macro for parsing actions DSL syntax
///
/// This macro allows you to write Rust-like syntax for defining actions:
///
/// ```ignore
/// actions_dsl! {
///     if get_visibility("modal") == Visibility::Hidden {
///         show("modal");
///         log("Modal shown");
///     } else {
///         hide("modal");
///     }
/// }
/// ```
#[proc_macro]
pub fn actions_dsl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input2 = proc_macro2::TokenStream::from(input);

    match expand_actions_dsl(input2) {
        Ok(tokens) => tokens.into(),
        Err(error_msg) => quote! {
            compile_error!(#error_msg)
        }
        .into(),
    }
}

fn expand_actions_dsl(input: TokenStream) -> Result<TokenStream, String> {
    // Parse the input tokens into our DSL AST
    let dsl = match Parser::parse2(parser::parse_dsl, input) {
        Ok(dsl) => dsl,
        Err(err) => {
            return Err(format!(
                "Actions DSL syntax error.\n\
                \n\
                Parsing failed: {err}\n\
                \n\
                The DSL supports Rust-like syntax including:\n\
                - Function calls: hide(\"id\"), show(\"id\"), log(\"message\")\n\
                - Variables: let visible = get_visibility(\"modal\");\n\
                - If statements: if condition {{ ... }} else {{ ... }}\n\
                - Match expressions: match value {{ pattern => action }}\n\
                - Method chaining: get_visibility(\"id\").eq(visible()).then(hide(\"id\"))\n\
                - Binary operations: a + b, a == b, a && b"
            ));
        }
    };

    // Generate the code that evaluates the DSL at runtime
    let output_ident = Ident::new("__hyperchad_actions_dsl_output", Span::mixed_site());

    // Generate the evaluation code, passing the output variable name
    let eval_code = evaluator::generate_evaluation_code(&dsl, &output_ident)?;

    Ok(quote! {{
        use hyperchad_actions::prelude::*;
        {
            let mut #output_ident: Vec<hyperchad_actions::ActionEffect> = Vec::new();

            #eval_code

            #output_ident
        }
    }})
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn test_simple_function_call() {
        let input = quote! {
            hide("test");
        };

        let result = expand_actions_dsl(input);
        assert!(result.is_ok(), "DSL should parse simple function call");
    }

    #[test]
    fn test_if_statement() {
        let input = quote! {
            if true {
                show("modal");
            } else {
                hide("modal");
            }
        };

        let result = expand_actions_dsl(input);
        assert!(result.is_ok(), "DSL should parse if statement");
    }

    #[test]
    fn test_variable_assignment() {
        let input = quote! {
            let target = "modal";
            show(target);
        };

        let result = expand_actions_dsl(input);
        assert!(result.is_ok(), "DSL should parse variable assignment");
    }
}
