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

/// Internal: Transforms select macro input into a fused futures select call.
///
/// This function parses the input tokens as `SelectWithPathInput`, extracts the crate path
/// and branches, wraps each future with `.fuse()` for safe polling, and generates either
/// a `select!` or `select_biased!` macro call depending on the input configuration.
///
/// This provides 100% `tokio::select!` compatibility while automatically fusing
/// futures/streams for the simulator runtime.
///
/// **Note:** This is an internal implementation detail of `switchy_async` and should not
/// be called directly. Use the `select!` macro from `switchy_async` instead.
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

/// Internal: Transforms join macro input into a futures join call.
///
/// This function parses the input tokens as `JoinWithPathInput`, extracts the crate path
/// and list of futures, and generates a `futures::join!` macro call with the provided futures.
///
/// This provides 100% `tokio::join!` compatibility for the simulator runtime.
///
/// **Note:** This is an internal implementation detail of `switchy_async` and should not
/// be called directly. Use the `join!` macro from `switchy_async` instead.
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

/// Internal: Transforms `try_join` macro input into a futures `try_join` call.
///
/// This function parses the input tokens as `JoinWithPathInput`, extracts the crate path
/// and list of futures, and generates a `futures::try_join!` macro call with the provided futures.
///
/// This provides 100% `tokio::try_join!` compatibility for the simulator runtime.
///
/// **Note:** This is an internal implementation detail of `switchy_async` and should not
/// be called directly. Use the `try_join!` macro from `switchy_async` instead.
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

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    /// Tests parsing of `SelectInput` with a single branch.
    #[::core::prelude::v1::test]
    fn select_input_single_branch() {
        let input = quote! {
            result = async_operation() => {
                println!("Operation completed: {:?}", result);
            }
        };
        let select: SelectInput = syn::parse2(input).expect("Failed to parse SelectInput");

        assert!(!select.biased, "Should not be biased");
        assert_eq!(select.branches.len(), 1, "Should have one branch");
    }

    /// Tests parsing of `SelectInput` with multiple branches.
    #[::core::prelude::v1::test]
    fn select_input_multiple_branches() {
        let input = quote! {
            result1 = future1() => { handle1(); },
            result2 = future2() => { handle2(); },
            result3 = future3() => { handle3(); }
        };
        let select: SelectInput = syn::parse2(input).expect("Failed to parse SelectInput");

        assert!(!select.biased, "Should not be biased");
        assert_eq!(select.branches.len(), 3, "Should have three branches");
    }

    /// Tests parsing of `SelectInput` with biased selection.
    #[::core::prelude::v1::test]
    fn select_input_biased() {
        let input = quote! {
            biased;
            result1 = future1() => { handle1(); },
            result2 = future2() => { handle2(); }
        };
        let select: SelectInput = syn::parse2(input).expect("Failed to parse biased SelectInput");

        assert!(select.biased, "Should be biased");
        assert_eq!(select.branches.len(), 2, "Should have two branches");
    }

    /// Tests parsing of `SelectBranch` with a guard condition.
    #[::core::prelude::v1::test]
    fn select_branch_with_guard() {
        let input = quote! {
            result = future(), if result.is_ok() => { handle(); }
        };
        let branch: SelectBranch = syn::parse2(input).expect("Failed to parse SelectBranch");

        assert!(
            branch.guard.is_some(),
            "Branch should have a guard condition"
        );
    }

    /// Tests parsing of `SelectBranch` without a guard condition.
    #[::core::prelude::v1::test]
    fn select_branch_without_guard() {
        let input = quote! {
            value = some_future() => { process(value); }
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch without guard");

        assert!(branch.guard.is_none(), "Branch should not have a guard");
    }

    /// Tests parsing of `SelectBranch` with a block handler.
    #[::core::prelude::v1::test]
    fn select_branch_block_handler() {
        let input = quote! {
            x = compute() => {
                println!("Got: {}", x);
                process(x);
            }
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch with block");

        // The handler should contain the block tokens
        let handler_str = branch.handler.to_string();
        assert!(
            handler_str.contains("println"),
            "Handler should contain block code"
        );
    }

    /// Tests parsing of `SelectBranch` with an expression handler.
    #[::core::prelude::v1::test]
    fn select_branch_expression_handler() {
        let input = quote! {
            result = operation() => result.unwrap()
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch with expression");

        let handler_str = branch.handler.to_string();
        assert!(
            handler_str.contains("unwrap"),
            "Handler should contain expression"
        );
    }

    /// Tests parsing of `SelectWithPathInput` with a basic path and branches.
    #[::core::prelude::v1::test]
    fn select_with_path_input_basic() {
        let input = quote! {
            @path = crate;
            value = future1() => { handle(); }
        };
        let parsed: SelectWithPathInput =
            syn::parse2(input).expect("Failed to parse SelectWithPathInput");

        assert_eq!(
            parsed.crate_path.segments.last().unwrap().ident.to_string(),
            "crate"
        );
        assert_eq!(parsed.select_input.branches.len(), 1);
    }

    /// Tests parsing of `SelectWithPathInput` with custom path and biased selection.
    #[::core::prelude::v1::test]
    fn select_with_path_input_biased() {
        let input = quote! {
            @path = switchy_async::futures;
            biased;
            a = fut_a() => {},
            b = fut_b() => {}
        };
        let parsed: SelectWithPathInput =
            syn::parse2(input).expect("Failed to parse SelectWithPathInput with biased");

        assert!(parsed.select_input.biased);
        assert_eq!(parsed.select_input.branches.len(), 2);
    }

    /// Tests parsing of `JoinWithPathInput` with a single future.
    #[::core::prelude::v1::test]
    fn join_with_path_input_single() {
        let input = quote! {
            @path = crate;
            future1()
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with single future");

        assert_eq!(parsed.futures.len(), 1, "Should have one future");
    }

    /// Tests parsing of `JoinWithPathInput` with multiple futures.
    #[::core::prelude::v1::test]
    fn join_with_path_input_multiple() {
        let input = quote! {
            @path = switchy_async;
            async { 1 },
            async { 2 },
            async { 3 },
            async { 4 }
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with multiple futures");

        assert_eq!(parsed.futures.len(), 4, "Should have four futures");
    }

    /// Tests parsing of `JoinWithPathInput` with complex future expressions.
    #[::core::prelude::v1::test]
    fn join_with_path_input_complex_futures() {
        let input = quote! {
            @path = crate;
            fetch_user(id),
            fetch_posts(user_id),
            calculate_stats()
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with complex futures");

        assert_eq!(parsed.futures.len(), 3, "Should have three futures");
    }

    /// Tests parsing of `JoinWithPathInput` with trailing comma.
    #[::core::prelude::v1::test]
    fn join_with_path_input_trailing_comma() {
        let input = quote! {
            @path = my_crate::runtime;
            future1(),
            future2(),
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with trailing comma");

        assert_eq!(parsed.futures.len(), 2, "Should have two futures");
    }

    /// Tests that `SelectWithPathInput` requires `@path` parameter.
    #[::core::prelude::v1::test]
    fn select_with_path_input_missing_path() {
        let input = quote! {
            result = future() => {}
        };
        let result = syn::parse2::<SelectWithPathInput>(input);

        assert!(
            result.is_err(),
            "Parsing should fail without @path parameter"
        );
    }

    /// Tests that `JoinWithPathInput` requires `@path` parameter.
    #[::core::prelude::v1::test]
    fn join_with_path_input_missing_path() {
        let input = quote! {
            future1(), future2()
        };
        let result = syn::parse2::<JoinWithPathInput>(input);

        assert!(
            result.is_err(),
            "Parsing should fail without @path parameter"
        );
    }

    /// Tests parsing of `SelectInput` with complex patterns.
    #[::core::prelude::v1::test]
    fn select_input_complex_patterns() {
        let input = quote! {
            Ok(value) = result_future() => { process(value); },
            Some(data) = option_future() => { handle(data); },
            _ = default_future() => { fallback(); }
        };
        let select: SelectInput =
            syn::parse2(input).expect("Failed to parse SelectInput with complex patterns");

        assert_eq!(select.branches.len(), 3, "Should have three branches");
    }

    /// Tests that empty `JoinWithPathInput` (no futures) is handled.
    #[::core::prelude::v1::test]
    fn join_with_path_input_empty() {
        let input = quote! {
            @path = crate;
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse empty JoinWithPathInput");

        assert_eq!(
            parsed.futures.len(),
            0,
            "Should have zero futures (though unusual)"
        );
    }

    // ============================================================================
    // Additional SelectBranch edge cases
    // ============================================================================

    /// Tests parsing of `SelectBranch` with tuple pattern.
    #[::core::prelude::v1::test]
    fn select_branch_tuple_pattern() {
        let input = quote! {
            (a, b) = get_pair() => { process(a, b); }
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch with tuple pattern");

        let pattern = branch.pattern;
        let pattern_str = quote!(#pattern).to_string();
        assert!(
            pattern_str.contains('a') && pattern_str.contains('b'),
            "Pattern should contain tuple elements: {pattern_str}"
        );
    }

    /// Tests parsing of `SelectBranch` with struct pattern.
    #[::core::prelude::v1::test]
    fn select_branch_struct_pattern() {
        let input = quote! {
            MyStruct { field } = get_struct() => { use_field(field); }
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch with struct pattern");

        let pattern = branch.pattern;
        let pattern_str = quote!(#pattern).to_string();
        assert!(
            pattern_str.contains("MyStruct"),
            "Pattern should contain struct name: {pattern_str}"
        );
    }

    /// Tests parsing of `SelectBranch` with nested block in handler.
    #[::core::prelude::v1::test]
    fn select_branch_nested_blocks() {
        let input = quote! {
            value = future() => {
                if value > 0 {
                    process_positive(value);
                } else {
                    process_negative(value);
                }
            }
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch with nested blocks");

        let handler_str = branch.handler.to_string();
        assert!(
            handler_str.contains("process_positive"),
            "Handler should contain nested if block: {handler_str}"
        );
    }

    /// Tests parsing of `SelectBranch` with complex guard expression.
    #[::core::prelude::v1::test]
    fn select_branch_complex_guard() {
        let input = quote! {
            result = future(), if result.is_ok() && result.as_ref().unwrap() > &0 => { handle(); }
        };
        let branch: SelectBranch =
            syn::parse2(input).expect("Failed to parse SelectBranch with complex guard");

        assert!(branch.guard.is_some(), "Branch should have a guard");
        let guard = branch.guard;
        let guard_str = quote!(#guard).to_string();
        assert!(
            guard_str.contains("&&"),
            "Guard should contain complex condition: {guard_str}"
        );
    }

    // ============================================================================
    // SelectInput edge cases
    // ============================================================================

    /// Tests parsing of `SelectInput` with empty branches (edge case).
    #[::core::prelude::v1::test]
    fn select_input_empty() {
        let input = quote! {};
        let select: SelectInput = syn::parse2(input).expect("Failed to parse empty SelectInput");

        assert!(!select.biased, "Should not be biased");
        assert_eq!(select.branches.len(), 0, "Should have no branches");
    }

    /// Tests parsing of `SelectInput` biased with no branches.
    #[::core::prelude::v1::test]
    fn select_input_biased_empty() {
        let input = quote! { biased; };
        let select: SelectInput =
            syn::parse2(input).expect("Failed to parse biased SelectInput with no branches");

        assert!(select.biased, "Should be biased");
        assert_eq!(select.branches.len(), 0, "Should have no branches");
    }

    /// Tests parsing of `SelectInput` with async block futures.
    #[::core::prelude::v1::test]
    fn select_input_async_block_futures() {
        let input = quote! {
            x = async { compute().await } => { handle(x); },
            y = async move { other().await } => { handle(y); }
        };
        let select: SelectInput =
            syn::parse2(input).expect("Failed to parse SelectInput with async blocks");

        assert_eq!(select.branches.len(), 2, "Should have two branches");
    }

    // ============================================================================
    // SelectWithPathInput edge cases
    // ============================================================================

    /// Tests parsing of `SelectWithPathInput` with deeply qualified path.
    #[::core::prelude::v1::test]
    fn select_with_path_input_deep_path() {
        let input = quote! {
            @path = crate::runtime::futures::ext;
            value = future() => {}
        };
        let parsed: SelectWithPathInput =
            syn::parse2(input).expect("Failed to parse SelectWithPathInput with deep path");

        assert_eq!(parsed.crate_path.segments.len(), 4);
        assert_eq!(parsed.crate_path.segments[0].ident.to_string(), "crate");
        assert_eq!(parsed.crate_path.segments[1].ident.to_string(), "runtime");
        assert_eq!(parsed.crate_path.segments[2].ident.to_string(), "futures");
        assert_eq!(parsed.crate_path.segments[3].ident.to_string(), "ext");
    }

    /// Tests parsing of `SelectWithPathInput` with many branches.
    #[::core::prelude::v1::test]
    fn select_with_path_input_many_branches() {
        let input = quote! {
            @path = crate;
            a = f1() => {},
            b = f2() => {},
            c = f3() => {},
            d = f4() => {},
            e = f5() => {}
        };
        let parsed: SelectWithPathInput =
            syn::parse2(input).expect("Failed to parse SelectWithPathInput with many branches");

        assert_eq!(parsed.select_input.branches.len(), 5);
    }

    // ============================================================================
    // JoinWithPathInput edge cases
    // ============================================================================

    /// Tests parsing of `JoinWithPathInput` with method chain futures.
    #[::core::prelude::v1::test]
    fn join_with_path_input_method_chains() {
        let input = quote! {
            @path = crate;
            client.get(url).send(),
            cache.fetch(key).map(Result::unwrap)
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with method chains");

        assert_eq!(parsed.futures.len(), 2, "Should have two futures");
    }

    /// Tests parsing of `JoinWithPathInput` with closure futures.
    #[::core::prelude::v1::test]
    fn join_with_path_input_closure_futures() {
        let input = quote! {
            @path = crate;
            (|| async { compute() })(),
            async move { work() }
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with closures");

        assert_eq!(parsed.futures.len(), 2, "Should have two futures");
    }

    /// Tests parsing of `JoinWithPathInput` with deeply qualified path.
    #[::core::prelude::v1::test]
    fn join_with_path_input_deep_path() {
        let input = quote! {
            @path = my_crate::runtime::executor;
            future1()
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with deep path");

        assert_eq!(parsed.crate_path.segments.len(), 3);
        assert_eq!(parsed.crate_path.segments[0].ident.to_string(), "my_crate");
        assert_eq!(parsed.crate_path.segments[1].ident.to_string(), "runtime");
        assert_eq!(parsed.crate_path.segments[2].ident.to_string(), "executor");
    }

    /// Tests parsing of `JoinWithPathInput` with many futures.
    #[::core::prelude::v1::test]
    fn join_with_path_input_many_futures() {
        let input = quote! {
            @path = crate;
            f1(),
            f2(),
            f3(),
            f4(),
            f5(),
            f6(),
            f7(),
            f8()
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with many futures");

        assert_eq!(parsed.futures.len(), 8, "Should have eight futures");
    }

    /// Tests parsing of `JoinWithPathInput` with nested async blocks.
    #[::core::prelude::v1::test]
    fn join_with_path_input_nested_async() {
        let input = quote! {
            @path = crate;
            async { async { inner().await }.await }
        };
        let parsed: JoinWithPathInput =
            syn::parse2(input).expect("Failed to parse JoinWithPathInput with nested async");

        assert_eq!(parsed.futures.len(), 1, "Should have one future");
    }
}
