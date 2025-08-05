use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::{
    Expr, Pat, Token,
    parse::{Parse, ParseStream, Result as ParseResult},
    parse_macro_input,
};

// ============================================================================
// select! macro implementation for simulator mode
// ============================================================================

/// A select! macro that provides 100% `tokio::select`! compatibility
/// while automatically fusing futures/streams for the simulator runtime
pub fn select(input: TokenStream) -> TokenStream {
    let select_input = parse_macro_input!(input as SelectInput);

    // Transform each branch to auto-fuse futures/streams
    let transformed_branches: Vec<_> = select_input
        .branches
        .into_iter()
        .map(|branch| {
            let SelectBranch {
                pattern,
                future,
                handler,
                guard,
            } = branch;

            // Wrap the future with .fuse() - this handles both futures and streams
            let fused_future = quote! { ::futures::FutureExt::fuse(#future) };

            // Reconstruct the branch with the fused future
            guard.map_or_else(
                || quote! { #pattern = #fused_future => #handler },
                |guard_expr| quote! { #pattern = #fused_future, if #guard_expr => #handler },
            )
        })
        .collect();

    // Handle biased selection if present
    let output = if select_input.biased {
        quote! {
            ::futures::select_biased! {
                #(#transformed_branches,)*
            }
        }
    } else {
        quote! {
            ::futures::select! {
                #(#transformed_branches,)*
            }
        }
    };

    output.into()
}

/// Represents the parsed input to a select! macro
struct SelectInput {
    biased: bool,
    branches: Vec<SelectBranch>,
}

/// Represents a single branch in a select! macro
struct SelectBranch {
    pattern: Pat,
    future: Expr,
    handler: TokenStream2,
    guard: Option<Expr>,
}

impl Parse for SelectInput {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        let mut biased = false;
        let mut branches = Vec::new();

        // Check for biased keyword
        if input.peek(syn::Ident) {
            let ident: syn::Ident = input.fork().parse()?;
            if ident == "biased" {
                let _: syn::Ident = input.parse()?;
                let _: Token![;] = input.parse()?;
                biased = true;
            }
        }

        // Parse branches
        while !input.is_empty() {
            let branch = input.parse::<SelectBranch>()?;
            branches.push(branch);

            // Handle optional trailing comma
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(Self { biased, branches })
    }
}

impl Parse for SelectBranch {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        // Parse: pattern = future [, if guard] => handler
        let pattern = Pat::parse_single(input)?;
        let _: Token![=] = input.parse()?;
        let future: Expr = input.parse()?;

        // Check for optional guard condition
        let guard = if input.peek(Token![,]) && input.peek2(Token![if]) {
            let _: Token![,] = input.parse()?;
            let _: Token![if] = input.parse()?;
            Some(input.parse::<Expr>()?)
        } else {
            None
        };

        let _: Token![=>] = input.parse()?;

        // Parse the handler - this can be an expression or a block
        let handler = if input.peek(syn::token::Brace) {
            // Parse as a block
            let block: syn::Block = input.parse()?;
            block.to_token_stream()
        } else {
            // Parse as an expression
            let expr: Expr = input.parse()?;
            expr.to_token_stream()
        };

        Ok(Self {
            pattern,
            future,
            handler,
            guard,
        })
    }
}
