//! Simulator-specific macro implementations.
//!
//! This module provides the internal implementations for `select!`, `join!`, and `try_join!`
//! macros when the simulator feature is enabled. These implementations provide 100% compatibility
//! with their tokio counterparts while integrating with the simulator runtime.

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

/// Internal select! macro that accepts a crate path parameter
/// This provides 100% `tokio::select`! compatibility while automatically
/// fusing futures/streams for the simulator runtime
pub fn select_internal(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as SelectWithPathInput);
    let crate_path = input.crate_path;
    let select_input = input.select_input;

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

            // Wrap the future with .fuse() using the provided crate path
            let fused_future = quote! { #crate_path::futures::FutureExt::fuse(#future) };

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
            #crate_path::futures::select_biased! {
                #(#transformed_branches,)*
            }
        }
    } else {
        quote! {
            #crate_path::futures::select! {
                #(#transformed_branches,)*
            }
        }
    };

    output.into()
}

/// Represents the parsed input to a `select!` macro with crate path.
///
/// This struct captures both the crate path for generating qualified paths
/// and the parsed select branches.
struct SelectWithPathInput {
    crate_path: syn::Path,
    select_input: SelectInput,
}

/// Represents the parsed input to a `select!` macro.
///
/// Contains the biased flag and all branches to be selected over.
struct SelectInput {
    biased: bool,
    branches: Vec<SelectBranch>,
}

/// Represents a single branch in a `select!` macro.
///
/// Each branch consists of a pattern, future expression, handler code,
/// and an optional guard condition.
struct SelectBranch {
    pattern: Pat,
    future: Expr,
    handler: TokenStream2,
    guard: Option<Expr>,
}

impl Parse for SelectWithPathInput {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        // Parse: (@path ::some::path) (normal_select_syntax)
        let _: Token![@] = input.parse()?;
        let _path_ident: syn::Ident = input.parse()?; // Should be "path"
        let _: Token![=] = input.parse()?;
        let crate_path: syn::Path = input.parse()?;
        let _: Token![;] = input.parse()?;
        let select_input: SelectInput = input.parse()?;

        Ok(Self {
            crate_path,
            select_input,
        })
    }
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

// ============================================================================
// join! and try_join! macro implementations for simulator mode
// ============================================================================

/// Internal join! macro that accepts a crate path parameter
/// This provides 100% `tokio::join!` compatibility for the simulator runtime
pub fn join_internal(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as JoinWithPathInput);
    let crate_path = input.crate_path;
    let futures = input.futures;

    // Generate the futures::join! call
    let output = quote! {
        #crate_path::futures::join!(#(#futures),*)
    };

    output.into()
}

/// Internal `try_join`! macro that accepts a crate path parameter
/// This provides 100% `tokio::try_join!` compatibility for the simulator runtime
pub fn try_join_internal(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as JoinWithPathInput);
    let crate_path = input.crate_path;
    let futures = input.futures;

    // Generate the futures::try_join! call
    let output = quote! {
        #crate_path::futures::try_join!(#(#futures),*)
    };

    output.into()
}

/// Represents the parsed input to a `join!/try_join!` macro with crate path.
///
/// Contains the crate path for generating qualified paths and the list
/// of futures to join.
struct JoinWithPathInput {
    crate_path: syn::Path,
    futures: Vec<Expr>,
}

impl Parse for JoinWithPathInput {
    fn parse(input: ParseStream) -> ParseResult<Self> {
        // Parse: (@path ::some::path) future1, future2, ...
        let _: Token![@] = input.parse()?;
        let _path_ident: syn::Ident = input.parse()?; // Should be "path"
        let _: Token![=] = input.parse()?;
        let crate_path: syn::Path = input.parse()?;
        let _: Token![;] = input.parse()?;

        let mut futures = Vec::new();
        while !input.is_empty() {
            futures.push(input.parse::<Expr>()?);
            if !input.is_empty() {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(Self {
            crate_path,
            futures,
        })
    }
}
