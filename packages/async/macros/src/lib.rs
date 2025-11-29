//! Procedural macros for async function transformation and yield injection.
//!
//! This crate provides macros that transform async functions to inject yield points after every
//! `.await`, enabling deterministic testing with the simulator runtime. It also provides
//! simulator-aware test attribute macros for setting up test runtimes.
//!
//! # Features
//!
//! * **Yield Injection**: Automatic yield point insertion via `#[inject_yields]` and `inject_yields_mod!`
//! * **Test Macros**: Simulator-aware test attributes (`#[test]`, `#[unsync_test]`, `#[tokio_test_wrapper]`)
//! * **Feature-Gated**: Transformation only occurs when the `simulator` feature is enabled
//! * **Zero Cost**: No runtime overhead when the simulator feature is disabled
//!
//! # Main Macros
//!
//! * [`inject_yields`] - Attribute macro for injecting yields into async functions
//! * [`inject_yields_mod`] - Procedural macro for transforming entire modules
//! * [`test`] - Test attribute for `switchy_async` tests (simulator feature only)
//! * [`unsync_test`] - Test attribute for `switchy::unsync` tests (simulator feature only)
//! * [`tokio_test_wrapper`] - Tokio-compatible test wrapper (always available)
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "simulator")]
//! # {
//! use switchy_async_macros::inject_yields;
//!
//! #[inject_yields]
//! async fn my_async_function(x: i32) -> i32 {
//!     // With the simulator feature enabled, yield points are automatically
//!     // inserted after each .await for deterministic execution
//!     x + 1
//! }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::PathBuf;
use std::str::FromStr as _;

use proc_macro::TokenStream;

#[cfg(feature = "simulator")]
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::visit_mut::{VisitMut, visit_expr_mut};
use syn::{Expr, ImplItem, Item, ItemMod, parse_macro_input};

#[cfg(feature = "simulator")]
mod simulator;

/// Represents the parsed input to a test macro with crate path.
///
/// This struct captures the complete configuration for a simulator test,
/// including the runtime path and optional flags for real-time, real-fs,
/// and simulator disabling.
#[cfg(feature = "simulator")]
struct TestWithPathInput {
    crate_path: syn::Path,
    use_real_time: bool,
    use_real_fs: bool,
    no_simulator: bool,
    function: syn::ItemFn,
}

#[cfg(feature = "simulator")]
impl syn::parse::Parse for TestWithPathInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        use syn::Token;

        // Parse: (@path ::some::path [; real_time] [; real_fs]) function_definition
        let _: Token![@] = input.parse()?;
        let _path_ident: syn::Ident = input.parse()?; // Should be "path"
        let _: Token![=] = input.parse()?;
        let crate_path: syn::Path = input.parse()?;
        let _: Token![;] = input.parse()?;

        // Check for optional real_time, real_fs, and no_simulator parameters
        let mut use_real_time = false;
        let mut use_real_fs = false;
        let mut no_simulator = false;

        while input.peek(syn::Ident) {
            let ident: syn::Ident = input.parse()?;
            if ident == "real_time" {
                use_real_time = true;
            } else if ident == "real_fs" {
                use_real_fs = true;
            } else if ident == "no_simulator" {
                no_simulator = true;
            } else {
                return Err(syn::Error::new_spanned(
                    ident,
                    "Expected 'real_time', 'real_fs', or 'no_simulator'",
                ));
            }
            let _: Token![;] = input.parse()?;
        }

        let function: syn::ItemFn = input.parse()?;

        Ok(Self {
            crate_path,
            use_real_time,
            use_real_fs,
            no_simulator,
            function,
        })
    }
}

/// Represents the parsed arguments for test attributes.
///
/// These arguments control the runtime behavior of simulator tests,
/// allowing tests to opt into real resources or disable simulation.
#[cfg(feature = "simulator")]
struct TestArgs {
    real_time: bool,
    real_fs: bool,
    no_simulator: bool,
}

#[cfg(feature = "simulator")]
impl syn::parse::Parse for TestArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut real_time = false;
        let mut real_fs = false;
        let mut no_simulator = false;

        // Parse comma-separated identifiers
        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            match ident.to_string().as_str() {
                "real_time" => real_time = true,
                "real_fs" => real_fs = true,
                "no_simulator" => no_simulator = true,
                _ => {
                    return Err(syn::Error::new_spanned(
                        &ident,
                        format!(
                            "Unknown test attribute: '{ident}'. Valid attributes are: 'real_time', 'real_fs', 'no_simulator'"
                        ),
                    ));
                }
            }

            // Check for comma (optional for last argument)
            if !input.is_empty() {
                let _: syn::Token![,] = input.parse()?;
            }
        }

        Ok(Self {
            real_time,
            real_fs,
            no_simulator,
        })
    }
}

/// Helper function to convert `TestArgs` to the internal token format.
///
/// Constructs the token stream that will be passed to `test_internal`,
/// combining the crate path, test arguments, and function tokens.
#[cfg(feature = "simulator")]
#[must_use]
fn build_test_tokens(
    crate_path: &str,
    args: &TestArgs,
    item_tokens: &TokenStream2,
) -> TokenStream2 {
    let mut tokens = if crate_path == "crate" {
        quote! { @path = crate; }
    } else {
        let path: syn::Path = syn::parse_str(crate_path).expect("Invalid crate path");
        quote! { @path = #path; }
    };

    if args.real_time {
        tokens.extend(quote! { real_time; });
    }
    if args.real_fs {
        tokens.extend(quote! { real_fs; });
    }
    if args.no_simulator {
        tokens.extend(quote! { no_simulator; });
    }

    tokens.extend(quote! { #item_tokens });
    tokens
}

/// Internal select! macro that accepts a crate path parameter.
///
/// This macro provides 100% `tokio::select!` compatibility while automatically
/// fusing futures/streams for the simulator runtime. It is used internally by the
/// `switchy_async` crate's `select!` macro implementation.
///
/// The macro accepts a special syntax with a crate path followed by the standard
/// `select!` branches:
///
/// ```ignore
/// select_internal! {
///     @path = crate_path;
///     pattern1 = future1 => handler1,
///     pattern2 = future2 => handler2,
/// }
/// ```
///
/// Most users should use the public `select!` macro from `switchy_async` rather than
/// calling this internal macro directly.
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn select_internal(input: TokenStream) -> TokenStream {
    simulator::select_internal(input)
}

/// Internal join! macro that accepts a crate path parameter.
///
/// This macro provides 100% `tokio::join!` compatibility for the simulator runtime.
/// It is used internally by the `switchy_async` crate's `join!` macro implementation.
///
/// The macro accepts a special syntax with a crate path followed by the futures to join:
///
/// ```ignore
/// join_internal! {
///     @path = crate_path;
///     future1,
///     future2,
///     future3
/// }
/// ```
///
/// Most users should use the public `join!` macro from `switchy_async` rather than
/// calling this internal macro directly.
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn join_internal(input: TokenStream) -> TokenStream {
    simulator::join_internal(input)
}

/// Internal `try_join!` macro that accepts a crate path parameter.
///
/// This macro provides 100% `tokio::try_join!` compatibility for the simulator runtime.
/// It is used internally by the `switchy_async` crate's `try_join!` macro implementation.
///
/// The macro accepts a special syntax with a crate path followed by the futures to join:
///
/// ```ignore
/// try_join_internal! {
///     @path = crate_path;
///     future1,
///     future2,
///     future3
/// }
/// ```
///
/// Like `tokio::try_join!`, this macro requires all futures to return `Result` types and
/// will short-circuit on the first error encountered.
///
/// Most users should use the public `try_join!` macro from `switchy_async` rather than
/// calling this internal macro directly.
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn try_join_internal(input: TokenStream) -> TokenStream {
    simulator::try_join_internal(input)
}

/// AST visitor that injects yield points after every `.await` expression.
///
/// This visitor walks through the syntax tree and transforms async await
/// expressions to include `yield_now()` calls for deterministic testing.
struct YieldInjector;

impl VisitMut for YieldInjector {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        visit_expr_mut(self, expr);

        if let Expr::Await(expr_await) = expr {
            let base = (*expr_await.base).clone();
            *expr = syn::parse_quote!({
                let __yield_res = #base.await;
                switchy::unsync::task::yield_now().await;
                __yield_res
            });
        }
    }
}

/// Recursively injects yield points into an AST item.
///
/// This function handles different item types (functions, impl blocks, modules)
/// and applies the yield injector to async functions within them.
fn inject_item(item: &mut Item, injector: &mut YieldInjector) {
    match item {
        Item::Fn(func) if func.sig.asyncness.is_some() => {
            injector.visit_block_mut(&mut func.block);
        }
        Item::Impl(item_impl) => {
            for impl_member in &mut item_impl.items {
                if let ImplItem::Fn(func) = impl_member
                    && func.sig.asyncness.is_some()
                {
                    injector.visit_block_mut(&mut func.block);
                }
            }
        }
        Item::Mod(item_mod) => {
            if let Some((_, items)) = &mut item_mod.content {
                for inner in items {
                    inject_item(inner, injector);
                }
            }
        }
        _ => {}
    }
}

/// Injects yield points after every `.await` in async functions when the `simulator` feature is enabled.
///
/// This macro transforms async functions to automatically call `switchy::unsync::task::yield_now().await`
/// after each await point, enabling deterministic testing with the simulator runtime.
///
/// When the `simulator` feature is disabled, this macro has no effect and returns the input unchanged.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::inject_yields;
///
/// #[inject_yields]
/// async fn fetch_data(url: &str) -> Result<String, std::io::Error> {
///     // Yield points are automatically inserted after each .await
///     let response = async { Ok("data".to_string()) }.await;
///     response
/// }
/// ```
#[allow(clippy::missing_const_for_fn)]
#[proc_macro_attribute]
pub fn inject_yields(_attr: TokenStream, item: TokenStream) -> TokenStream {
    #[cfg(not(feature = "simulator"))]
    {
        return item;
    }

    #[allow(unreachable_code)]
    {
        let mut ast = parse_macro_input!(item as Item);
        let mut injector = YieldInjector;
        inject_item(&mut ast, &mut injector);
        TokenStream::from(quote!(#ast))
    }
}

/// Internal test attribute macro that accepts a crate path parameter.
///
/// This macro provides test runtime setup for the simulator runtime. It is used internally
/// by the public test macros (`test`, `unsync_test`, `internal_test`) to generate the
/// appropriate test wrapper with the correct crate path.
///
/// The macro accepts a special syntax with configuration parameters:
///
/// ```ignore
/// test_internal! {
///     @path = crate_path;
///     [real_time;]
///     [real_fs;]
///     [no_simulator;]
///     async fn test_function() {
///         // test body
///     }
/// }
/// ```
///
/// Most users should use the public test attribute macros (`#[test]`, `#[unsync_test]`, etc.)
/// rather than calling this internal macro directly.
#[allow(clippy::too_many_lines)]
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn test_internal(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as TestWithPathInput);
    let crate_path = input.crate_path;
    let use_real_time = input.use_real_time;
    let use_real_fs = input.use_real_fs;
    let no_simulator = input.no_simulator;
    let input_fn = input.function;

    let fn_name = &input_fn.sig.ident;
    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;
    let fn_inputs = &input_fn.sig.inputs;
    let fn_output = &input_fn.sig.output;

    // Extract any existing attributes except #[test]
    let filtered_attrs: Vec<_> = fn_attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("test"))
        .collect();

    // If no_simulator is set and the macro was compiled with simulator enabled,
    // generate a function without the test attribute to skip it
    if no_simulator {
        // This cfg! check happens when the MACRO is compiled, not when it's used
        const SIMULATOR_ENABLED: bool = cfg!(feature = "simulator");

        if SIMULATOR_ENABLED {
            // Skip the test by generating a non-test function - preserve async if present
            let async_token = &input_fn.sig.asyncness;
            let result = quote! {
                #(#filtered_attrs)*
                #[allow(dead_code)]
                #fn_vis #async_token fn #fn_name(#fn_inputs) #fn_output #fn_block
            };
            return result.into();
        }

        // Generate a normal test function - handle async properly
        let result = if input_fn.sig.asyncness.is_some() {
            // Input is async - use body directly in block_on without extra async move wrapper
            quote! {
                #(#filtered_attrs)*
                #[::core::prelude::v1::test]
                #fn_vis fn #fn_name() {
                    let rt = #crate_path::Builder::new().build().unwrap();
                    rt.block_on(async move #fn_block)
                    // Don't call rt.wait() as it can hang in tests
                }
            }
        } else {
            // Input is sync - wrap in async move as before
            quote! {
                #(#filtered_attrs)*
                #[::core::prelude::v1::test]
                #fn_vis fn #fn_name(#fn_inputs) #fn_output {
                    let rt = #crate_path::Builder::new().build().unwrap();
                    rt.block_on(async move #fn_block)
                    // Don't call rt.wait() as it can hang in tests
                }
            }
        };

        return result.into();
    }

    // Determine the correct paths for fs and time modules based on crate_path
    let (fs_path, time_path) = if crate_path == syn::parse_quote!(switchy_async)
        || crate_path == syn::parse_quote!(crate)
    {
        // Direct invocation from switchy_async or internal tests
        // Try to use switchy umbrella crate first, but fall back to direct crate access
        // if switchy is not available (e.g., in packages that only depend on switchy_async)
        (quote!(switchy_fs), quote!(switchy_time))
    } else if let Some(first_segment) = crate_path.segments.first() {
        if first_segment.ident == "switchy" {
            // Any path starting with switchy (like switchy::unsync)
            // We need to use switchy::fs, not switchy::unsync::fs
            (quote!(switchy::fs), quote!(switchy::time))
        } else {
            // Some other crate that might have its own fs/time modules
            (quote!(#crate_path::fs), quote!(#crate_path::time))
        }
    } else {
        // Fallback - assume the crate_path has fs and time
        (quote!(#crate_path::fs), quote!(#crate_path::time))
    };

    let result = match (use_real_time, use_real_fs) {
        (true, true) => {
            quote! {
                #(#filtered_attrs)*
                #[::core::prelude::v1::test]
                #fn_vis fn #fn_name(#fn_inputs) #fn_output {
                    #time_path::simulator::with_real_time(|| {
                        #fs_path::simulator::with_real_fs(|| {
                            let rt = #crate_path::Builder::new().build().unwrap();
                            rt.block_on(async move #fn_block)
                            // Don't call rt.wait() as it can hang in tests
                        })
                    })
                }
            }
        }
        (true, false) => {
            quote! {
                #(#filtered_attrs)*
                #[::core::prelude::v1::test]
                #fn_vis fn #fn_name(#fn_inputs) #fn_output {
                    #time_path::simulator::with_real_time(|| {
                        let rt = #crate_path::Builder::new().build().unwrap();
                        rt.block_on(async move #fn_block)
                        // Don't call rt.wait() as it can hang in tests
                    })
                }
            }
        }
        (false, true) => {
            quote! {
                #(#filtered_attrs)*
                #[::core::prelude::v1::test]
                #fn_vis fn #fn_name(#fn_inputs) #fn_output {
                    #fs_path::simulator::with_real_fs(|| {
                        let rt = #crate_path::Builder::new().build().unwrap();
                        rt.block_on(async move #fn_block)
                        // Don't call rt.wait() as it can hang in tests
                    })
                }
            }
        }
        (false, false) => {
            quote! {
                #(#filtered_attrs)*
                #[::core::prelude::v1::test]
                #fn_vis fn #fn_name(#fn_inputs) #fn_output {
                    let rt = #crate_path::Builder::new().build().unwrap();
                    rt.block_on(async move #fn_block)
                    // Don't call rt.wait() as it can hang in tests
                }
            }
        }
    };

    result.into()
}

/// Internal test attribute macro for `switchy_async` - uses crate path for internal usage.
///
/// This macro is used within the `switchy_async` crate itself for internal testing. It sets up
/// a simulator runtime and runs the test function within it, using `crate` as the path.
///
/// Supports optional parameters:
/// * `real_time` - Use real time instead of simulated time
/// * `real_fs` - Use real filesystem instead of simulated filesystem
/// * `no_simulator` - Skip test when simulator feature is enabled, run normally otherwise
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::internal_test;
///
/// #[internal_test]
/// async fn my_internal_test() {
///     // Test code here
/// }
///
/// #[internal_test(real_time)]
/// async fn test_with_real_time() {
///     // Uses real time
/// }
/// ```
#[cfg(feature = "simulator")]
#[proc_macro_attribute]
pub fn internal_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let test_args = if args.is_empty() {
        TestArgs {
            real_time: false,
            real_fs: false,
            no_simulator: false,
        }
    } else {
        match syn::parse::<TestArgs>(args) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error().into(),
        }
    };

    let item_tokens: TokenStream2 = item.into();
    let input_tokens = build_test_tokens("crate", &test_args, &item_tokens);

    test_internal(input_tokens.into())
}

/// Test attribute macro for `switchy_async` runtime tests (simulator feature only).
///
/// This macro sets up a simulator runtime and runs the test function within it. It's designed
/// for testing code that uses the `switchy_async` runtime. Only available when the `simulator`
/// feature is enabled.
///
/// Supports optional parameters:
/// * `real_time` - Use real time instead of simulated time
/// * `real_fs` - Use real filesystem instead of simulated filesystem
/// * `no_simulator` - Skip test when simulator feature is enabled, run normally otherwise
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::test;
///
/// #[test]
/// async fn my_async_test() {
///     // Test code using switchy_async runtime
///     assert_eq!(2 + 2, 4);
/// }
///
/// #[test(real_time, real_fs)]
/// async fn test_with_real_resources() {
///     // Uses real time and filesystem
/// }
/// ```
#[cfg(feature = "simulator")]
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    let test_args = if args.is_empty() {
        TestArgs {
            real_time: false,
            real_fs: false,
            no_simulator: false,
        }
    } else {
        match syn::parse::<TestArgs>(args) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error().into(),
        }
    };

    let item_tokens: TokenStream2 = item.into();
    let input_tokens = build_test_tokens("switchy_async", &test_args, &item_tokens);

    test_internal(input_tokens.into())
}

/// Test attribute macro for `switchy::unsync` runtime tests (simulator feature only).
///
/// This macro sets up a `switchy::unsync` runtime and runs the test function within it. It's
/// designed for testing code that uses the `switchy::unsync` runtime. Only available when the
/// `simulator` feature is enabled.
///
/// Supports optional parameters:
/// * `real_time` - Use real time instead of simulated time
/// * `real_fs` - Use real filesystem instead of simulated filesystem
/// * `no_simulator` - Skip test when simulator feature is enabled, run normally otherwise
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::unsync_test;
///
/// #[unsync_test]
/// async fn my_unsync_test() {
///     // Test code using switchy::unsync runtime
///     assert_eq!(2 + 2, 4);
/// }
///
/// #[unsync_test(real_time)]
/// async fn test_with_real_time() {
///     // Uses real time for this test
/// }
/// ```
#[cfg(feature = "simulator")]
#[proc_macro_attribute]
pub fn unsync_test(args: TokenStream, item: TokenStream) -> TokenStream {
    let test_args = if args.is_empty() {
        TestArgs {
            real_time: false,
            real_fs: false,
            no_simulator: false,
        }
    } else {
        match syn::parse::<TestArgs>(args) {
            Ok(args) => args,
            Err(e) => return e.to_compile_error().into(),
        }
    };

    let item_tokens: TokenStream2 = item.into();
    let input_tokens = build_test_tokens("switchy::unsync", &test_args, &item_tokens);

    test_internal(input_tokens.into())
}

/// Tokio-compatible test attribute macro (always available).
///
/// This macro provides a Tokio-compatible test wrapper that simply delegates to `#[tokio::test]`.
/// Unlike the other test macros in this crate, this macro is always available regardless of
/// feature flags, making it suitable for tests that need to work with or without the simulator.
///
/// Any arguments passed to this macro are ignored - it's designed to be a drop-in replacement
/// for `#[tokio::test]` with a compatible signature.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::tokio_test_wrapper;
///
/// #[tokio_test_wrapper]
/// async fn my_tokio_test() {
///     // Test code using tokio runtime
///     assert_eq!(2 + 2, 4);
/// }
///
/// // Parameters are accepted but ignored for compatibility
/// #[tokio_test_wrapper(real_time)]
/// async fn test_with_ignored_args() {
///     // Still runs as a standard tokio test
/// }
/// ```
#[proc_macro_attribute]
pub fn tokio_test_wrapper(_args: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as syn::ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;
    let fn_output = &input_fn.sig.output;

    // Extract any existing attributes except #[test]
    let filtered_attrs: Vec<_> = fn_attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("test"))
        .collect();

    let result = quote! {
        #(#filtered_attrs)*
        #[::core::prelude::v1::test]
        #fn_vis fn #fn_name() #fn_output {
            let rt = ::switchy_async::Builder::new().build().expect("Failed to build runtime");
            rt.block_on(async move #fn_block)
            // Don't call rt.wait() as it can hang in tests
        }
    };

    result.into()
}

// ============================================================================
// main macro implementation
// ============================================================================

/// Represents the parsed input to a main macro with crate path.
///
/// This struct captures the complete configuration for a runtime main function,
/// including the runtime path and optional configuration parameters.
#[cfg(feature = "simulator")]
struct MainWithPathInput {
    crate_path: syn::Path,
    function: syn::ItemFn,
}

#[cfg(feature = "simulator")]
impl syn::parse::Parse for MainWithPathInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        use syn::Token;

        // Parse: @path = ::some::path; async fn main() { ... }
        let _: Token![@] = input.parse()?;
        let _path_ident: syn::Ident = input.parse()?; // Should be "path"
        let _: Token![=] = input.parse()?;
        let crate_path: syn::Path = input.parse()?;
        let _: Token![;] = input.parse()?;

        let function: syn::ItemFn = input.parse()?;

        Ok(Self {
            crate_path,
            function,
        })
    }
}

/// Represents the parsed arguments for main attributes.
///
/// Currently empty as we don't support any configuration parameters,
/// but this structure allows for future extensibility.
#[cfg(feature = "simulator")]
struct MainArgs;

#[cfg(feature = "simulator")]
impl syn::parse::Parse for MainArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        // Currently we don't support any parameters for main
        // Just consume and ignore any tokens for forward compatibility
        while !input.is_empty() {
            let _: proc_macro2::TokenTree = input.parse()?;
        }
        Ok(Self)
    }
}

/// Helper function to convert `MainArgs` to the internal token format.
///
/// Constructs the token stream that will be passed to `main_internal`,
/// combining the crate path and function tokens.
#[cfg(feature = "simulator")]
#[must_use]
fn build_main_tokens(crate_path: &str, item_tokens: &TokenStream2) -> TokenStream2 {
    let tokens = if crate_path == "crate" {
        quote! { @path = crate; }
    } else {
        let path: syn::Path = syn::parse_str(crate_path).expect("Invalid crate path");
        quote! { @path = #path; }
    };

    let mut result = tokens;
    result.extend(quote! { #item_tokens });
    result
}

/// Internal main attribute macro that accepts a crate path parameter.
///
/// This macro provides runtime setup for async main functions. It is used internally
/// by the public main macros (`main`, `unsync_main`, `internal_main`) to generate the
/// appropriate runtime wrapper with the correct crate path.
///
/// The macro accepts a special syntax with configuration parameters:
///
/// ```ignore
/// main_internal! {
///     @path = crate_path;
///     async fn main() {
///         // main body
///     }
/// }
/// ```
///
/// Most users should use the public main attribute macros (`#[main]`, `#[unsync_main]`, etc.)
/// rather than calling this internal macro directly.
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn main_internal(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as MainWithPathInput);
    let crate_path = input.crate_path;
    let input_fn = input.function;

    let fn_name = &input_fn.sig.ident;
    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;
    let fn_output = &input_fn.sig.output;

    // Extract any existing attributes except #[main]
    let filtered_attrs: Vec<_> = fn_attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("main"))
        .collect();

    // Determine if the return type is a Result type
    let is_result_type = matches!(fn_output, syn::ReturnType::Type(_, ty) if {
        if let syn::Type::Path(type_path) = ty.as_ref() {
            type_path.path.segments.last().is_some_and(|seg| seg.ident == "Result")
        } else {
            false
        }
    });

    let result = if is_result_type {
        // For Result return types, propagate the result properly
        quote! {
            #(#filtered_attrs)*
            #fn_vis fn #fn_name() #fn_output {
                let rt = #crate_path::Builder::new().build().expect("Failed to build runtime");
                let result = rt.block_on(async move #fn_block);
                rt.wait().expect("Runtime wait failed");
                result
            }
        }
    } else {
        // For non-Result return types (including unit)
        quote! {
            #(#filtered_attrs)*
            #fn_vis fn #fn_name() #fn_output {
                let rt = #crate_path::Builder::new().build().expect("Failed to build runtime");
                let result = rt.block_on(async move #fn_block);
                rt.wait().expect("Runtime wait failed");
                result
            }
        }
    };

    result.into()
}

/// Internal main attribute macro for `switchy_async` - uses crate path for internal usage.
///
/// This macro is used within the `switchy_async` crate itself for internal main functions.
/// It sets up a simulator runtime and runs the main function within it, using `crate` as the path.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::internal_main;
///
/// #[internal_main]
/// async fn main() {
///     // Main code here
/// }
/// ```
#[cfg(feature = "simulator")]
#[proc_macro_attribute]
pub fn internal_main(args: TokenStream, item: TokenStream) -> TokenStream {
    // Parse and ignore args for forward compatibility
    if !args.is_empty() {
        let _ = syn::parse::<MainArgs>(args);
    }

    let item_tokens: TokenStream2 = item.into();
    let input_tokens = build_main_tokens("crate", &item_tokens);

    main_internal(input_tokens.into())
}

/// Main attribute macro for `switchy_async` runtime (simulator feature only).
///
/// This macro sets up a simulator runtime and runs the main function within it. It's designed
/// for async main functions that use the `switchy_async` runtime. Only available when the
/// `simulator` feature is enabled.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::main;
///
/// #[main]
/// async fn main() {
///     // Async main code using switchy_async runtime
///     println!("Hello from async main!");
/// }
///
/// #[main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // With Result return type
///     Ok(())
/// }
/// ```
#[cfg(feature = "simulator")]
#[proc_macro_attribute]
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    // Parse and ignore args for forward compatibility
    if !args.is_empty() {
        let _ = syn::parse::<MainArgs>(args);
    }

    let item_tokens: TokenStream2 = item.into();
    let input_tokens = build_main_tokens("switchy_async", &item_tokens);

    main_internal(input_tokens.into())
}

/// Main attribute macro for `switchy::unsync` runtime (simulator feature only).
///
/// This macro sets up a `switchy::unsync` runtime and runs the main function within it. It's
/// designed for async main functions that use the `switchy::unsync` runtime. Only available
/// when the `simulator` feature is enabled.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::unsync_main;
///
/// #[unsync_main]
/// async fn main() {
///     // Async main code using switchy::unsync runtime
///     println!("Hello from async main!");
/// }
///
/// #[unsync_main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // With Result return type
///     Ok(())
/// }
/// ```
#[cfg(feature = "simulator")]
#[proc_macro_attribute]
pub fn unsync_main(args: TokenStream, item: TokenStream) -> TokenStream {
    // Parse and ignore args for forward compatibility
    if !args.is_empty() {
        let _ = syn::parse::<MainArgs>(args);
    }

    let item_tokens: TokenStream2 = item.into();
    let input_tokens = build_main_tokens("switchy::unsync", &item_tokens);

    main_internal(input_tokens.into())
}

/// Tokio-compatible main attribute macro (always available).
///
/// This macro provides a Tokio-compatible main wrapper that generates runtime setup code
/// using `switchy_async::Builder`. Unlike delegating to `#[tokio::main]`, this approach
/// doesn't require `tokio` as a direct dependency of the using crate.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_async_macros::tokio_main_wrapper;
///
/// #[tokio_main_wrapper]
/// async fn main() {
///     // Async main code using tokio runtime
///     println!("Hello from async main!");
/// }
///
/// #[tokio_main_wrapper]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // With Result return type
///     Ok(())
/// }
/// ```
#[proc_macro_attribute]
pub fn tokio_main_wrapper(_args: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as syn::ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_block = &input_fn.block;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;
    let fn_output = &input_fn.sig.output;

    // Extract any existing attributes except #[main]
    let filtered_attrs: Vec<_> = fn_attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("main"))
        .collect();

    // Determine if the return type is a Result type
    let is_result_type = matches!(fn_output, syn::ReturnType::Type(_, ty) if {
        if let syn::Type::Path(type_path) = ty.as_ref() {
            type_path.path.segments.last().is_some_and(|seg| seg.ident == "Result")
        } else {
            false
        }
    });

    let result = if is_result_type {
        // For Result return types, propagate the result properly
        quote! {
            #(#filtered_attrs)*
            #fn_vis fn #fn_name() #fn_output {
                let rt = ::switchy_async::Builder::new().build().expect("Failed to build runtime");
                let result = rt.block_on(async move #fn_block);
                rt.wait().expect("Runtime wait failed");
                result
            }
        }
    } else {
        // For non-Result return types (including unit)
        quote! {
            #(#filtered_attrs)*
            #fn_vis fn #fn_name() #fn_output {
                let rt = ::switchy_async::Builder::new().build().expect("Failed to build runtime");
                let result = rt.block_on(async move #fn_block);
                rt.wait().expect("Runtime wait failed");
                result
            }
        }
    };

    result.into()
}

/// Injects yield points into an entire module by reading and transforming the module's source file.
///
/// This macro reads the module file from disk, parses it, and applies yield injection to all async
/// functions within. When the `simulator` feature is disabled, returns the input unchanged.
///
/// # Panics
///
/// * If `CARGO_MANIFEST_DIR` environment variable is not set
/// * If the module source file path cannot be constructed or read
/// * If the module source file cannot be parsed as valid Rust code
#[allow(clippy::missing_const_for_fn)]
#[proc_macro]
pub fn inject_yields_mod(input: TokenStream) -> TokenStream {
    #[cfg(not(feature = "simulator"))]
    {
        return input;
    }

    #[allow(unreachable_code)]
    {
        let mod_decl: ItemMod = parse_macro_input!(input as ItemMod);
        let ident = &mod_decl.ident;
        let path = PathBuf::from_str(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .unwrap()
            .join("src")
            .join(format!("{ident}.rs"));
        let code = std::fs::read_to_string(path).unwrap();
        // parse the file’s AST, run your YieldInjector on it…
        let mut file = syn::parse_file(&code).unwrap();
        let mut injector = YieldInjector;
        injector.visit_file_mut(&mut file);
        // emit back: `pub mod x { /* transformed file.items */ }`
        let items = file.items;
        quote! {
            pub mod #ident {
                #(#items)*
            }
        }
        .into()
    }
}

#[cfg(all(test, feature = "simulator"))]
mod tests {
    use super::*;
    use quote::quote;

    // ============================================================================
    // TestArgs parsing tests
    // ============================================================================

    /// Tests parsing of empty `TestArgs` (no flags).
    #[::core::prelude::v1::test]
    fn test_args_empty() {
        let input = quote! {};
        let args: TestArgs = syn::parse2(input).expect("Failed to parse empty TestArgs");

        assert!(!args.real_time, "real_time should be false");
        assert!(!args.real_fs, "real_fs should be false");
        assert!(!args.no_simulator, "no_simulator should be false");
    }

    /// Tests parsing of `TestArgs` with single `real_time` flag.
    #[::core::prelude::v1::test]
    fn test_args_real_time_only() {
        let input = quote! { real_time };
        let args: TestArgs = syn::parse2(input).expect("Failed to parse TestArgs with real_time");

        assert!(args.real_time, "real_time should be true");
        assert!(!args.real_fs, "real_fs should be false");
        assert!(!args.no_simulator, "no_simulator should be false");
    }

    /// Tests parsing of `TestArgs` with single `real_fs` flag.
    #[::core::prelude::v1::test]
    fn test_args_real_fs_only() {
        let input = quote! { real_fs };
        let args: TestArgs = syn::parse2(input).expect("Failed to parse TestArgs with real_fs");

        assert!(!args.real_time, "real_time should be false");
        assert!(args.real_fs, "real_fs should be true");
        assert!(!args.no_simulator, "no_simulator should be false");
    }

    /// Tests parsing of `TestArgs` with single `no_simulator` flag.
    #[::core::prelude::v1::test]
    fn test_args_no_simulator_only() {
        let input = quote! { no_simulator };
        let args: TestArgs =
            syn::parse2(input).expect("Failed to parse TestArgs with no_simulator");

        assert!(!args.real_time, "real_time should be false");
        assert!(!args.real_fs, "real_fs should be false");
        assert!(args.no_simulator, "no_simulator should be true");
    }

    /// Tests parsing of `TestArgs` with multiple comma-separated flags.
    #[::core::prelude::v1::test]
    fn test_args_multiple_flags() {
        let input = quote! { real_time, real_fs };
        let args: TestArgs =
            syn::parse2(input).expect("Failed to parse TestArgs with multiple flags");

        assert!(args.real_time, "real_time should be true");
        assert!(args.real_fs, "real_fs should be true");
        assert!(!args.no_simulator, "no_simulator should be false");
    }

    /// Tests parsing of `TestArgs` with all flags enabled.
    #[::core::prelude::v1::test]
    fn test_args_all_flags() {
        let input = quote! { real_time, real_fs, no_simulator };
        let args: TestArgs = syn::parse2(input).expect("Failed to parse TestArgs with all flags");

        assert!(args.real_time, "real_time should be true");
        assert!(args.real_fs, "real_fs should be true");
        assert!(args.no_simulator, "no_simulator should be true");
    }

    /// Tests parsing of `TestArgs` with trailing comma.
    #[::core::prelude::v1::test]
    fn test_args_trailing_comma() {
        let input = quote! { real_time, };
        let args: TestArgs =
            syn::parse2(input).expect("Failed to parse TestArgs with trailing comma");

        assert!(args.real_time, "real_time should be true");
    }

    /// Tests that unknown attributes produce an error.
    #[::core::prelude::v1::test]
    fn test_args_unknown_attribute() {
        let input = quote! { unknown_flag };
        let result = syn::parse2::<TestArgs>(input);

        assert!(result.is_err(), "Unknown attribute should produce an error");
        // Verify error message contains expected text
        let err = result.err().expect("Expected error");
        assert!(
            err.to_string().contains("Unknown test attribute"),
            "Error message should mention unknown attribute"
        );
    }

    // ============================================================================
    // TestWithPathInput parsing tests
    // ============================================================================

    /// Tests parsing of `TestWithPathInput` with minimal valid input.
    #[::core::prelude::v1::test]
    fn test_with_path_input_basic() {
        let input = quote! {
            @path = crate;
            async fn my_test() {}
        };
        let parsed: TestWithPathInput =
            syn::parse2(input).expect("Failed to parse basic TestWithPathInput");

        assert_eq!(
            parsed.crate_path.segments.last().unwrap().ident.to_string(),
            "crate"
        );
        assert!(!parsed.use_real_time, "real_time should be false");
        assert!(!parsed.use_real_fs, "real_fs should be false");
        assert!(!parsed.no_simulator, "no_simulator should be false");
        assert_eq!(parsed.function.sig.ident.to_string(), "my_test");
    }

    /// Tests parsing of `TestWithPathInput` with `real_time` flag.
    #[::core::prelude::v1::test]
    fn test_with_path_input_real_time() {
        let input = quote! {
            @path = switchy_async;
            real_time;
            async fn test_with_real_time() {}
        };
        let parsed: TestWithPathInput =
            syn::parse2(input).expect("Failed to parse TestWithPathInput with real_time");

        assert!(parsed.use_real_time, "real_time should be true");
        assert!(!parsed.use_real_fs, "real_fs should be false");
        assert!(!parsed.no_simulator, "no_simulator should be false");
    }

    /// Tests parsing of `TestWithPathInput` with `real_fs` flag.
    #[::core::prelude::v1::test]
    fn test_with_path_input_real_fs() {
        let input = quote! {
            @path = switchy::unsync;
            real_fs;
            async fn test_with_real_fs() {}
        };
        let parsed: TestWithPathInput =
            syn::parse2(input).expect("Failed to parse TestWithPathInput with real_fs");

        assert!(!parsed.use_real_time, "real_time should be false");
        assert!(parsed.use_real_fs, "real_fs should be true");
        assert!(!parsed.no_simulator, "no_simulator should be false");
    }

    /// Tests parsing of `TestWithPathInput` with `no_simulator` flag.
    #[::core::prelude::v1::test]
    fn test_with_path_input_no_simulator() {
        let input = quote! {
            @path = my_crate;
            no_simulator;
            async fn test_no_sim() {}
        };
        let parsed: TestWithPathInput =
            syn::parse2(input).expect("Failed to parse TestWithPathInput with no_simulator");

        assert!(!parsed.use_real_time, "real_time should be false");
        assert!(!parsed.use_real_fs, "real_fs should be false");
        assert!(parsed.no_simulator, "no_simulator should be true");
    }

    /// Tests parsing of `TestWithPathInput` with multiple flags.
    #[::core::prelude::v1::test]
    fn test_with_path_input_multiple_flags() {
        let input = quote! {
            @path = switchy_async;
            real_time;
            real_fs;
            no_simulator;
            async fn test_all_flags() {}
        };
        let parsed: TestWithPathInput =
            syn::parse2(input).expect("Failed to parse TestWithPathInput with all flags");

        assert!(parsed.use_real_time, "real_time should be true");
        assert!(parsed.use_real_fs, "real_fs should be true");
        assert!(parsed.no_simulator, "no_simulator should be true");
    }

    /// Tests parsing of `TestWithPathInput` with qualified crate path.
    #[::core::prelude::v1::test]
    fn test_with_path_input_qualified_path() {
        let input = quote! {
            @path = switchy::unsync::runtime;
            async fn my_test() {}
        };
        let parsed: TestWithPathInput =
            syn::parse2(input).expect("Failed to parse TestWithPathInput with qualified path");

        assert_eq!(parsed.crate_path.segments.len(), 3);
        assert_eq!(parsed.crate_path.segments[0].ident.to_string(), "switchy");
        assert_eq!(parsed.crate_path.segments[1].ident.to_string(), "unsync");
        assert_eq!(parsed.crate_path.segments[2].ident.to_string(), "runtime");
    }

    /// Tests that invalid flag in `TestWithPathInput` produces error.
    #[::core::prelude::v1::test]
    fn test_with_path_input_invalid_flag() {
        let input = quote! {
            @path = crate;
            invalid_flag;
            async fn my_test() {}
        };
        let result = syn::parse2::<TestWithPathInput>(input);

        assert!(result.is_err(), "Invalid flag should produce an error");
    }

    // ============================================================================
    // YieldInjector tests
    // ============================================================================

    /// Tests that `YieldInjector` transforms simple await expressions.
    #[::core::prelude::v1::test]
    fn yield_injector_simple_await() {
        let input: Expr = syn::parse_quote! {
            some_future().await
        };

        let mut expr = input;
        let mut injector = YieldInjector;
        injector.visit_expr_mut(&mut expr);

        let output = quote!(#expr).to_string();
        assert!(
            output.contains("yield_now"),
            "Output should contain yield_now: {output}"
        );
        assert!(
            output.contains("__yield_res"),
            "Output should contain __yield_res temporary: {output}"
        );
    }

    /// Tests that `YieldInjector` transforms nested await expressions.
    #[::core::prelude::v1::test]
    fn yield_injector_nested_await() {
        let input: Expr = syn::parse_quote! {
            async_fn(another().await).await
        };

        let mut expr = input;
        let mut injector = YieldInjector;
        injector.visit_expr_mut(&mut expr);

        let output = quote!(#expr).to_string();
        // Both awaits should be transformed
        let yield_count = output.matches("yield_now").count();
        assert_eq!(yield_count, 2, "Should have two yield_now calls: {output}");
    }

    /// Tests that `YieldInjector` preserves non-await expressions.
    #[::core::prelude::v1::test]
    fn yield_injector_no_await() {
        let input: Expr = syn::parse_quote! {
            some_function(x, y)
        };

        let original = quote!(#input).to_string();
        let mut expr = input;
        let mut injector = YieldInjector;
        injector.visit_expr_mut(&mut expr);

        let output = quote!(#expr).to_string();
        assert_eq!(
            output, original,
            "Non-await expressions should not be modified"
        );
    }

    // ============================================================================
    // inject_item tests
    // ============================================================================

    /// Tests that `inject_item` processes async functions.
    #[::core::prelude::v1::test]
    fn inject_item_async_function() {
        let mut item: Item = syn::parse_quote! {
            async fn test_fn() {
                some_future().await;
            }
        };

        let mut injector = YieldInjector;
        inject_item(&mut item, &mut injector);

        let output = quote!(#item).to_string();
        assert!(
            output.contains("yield_now"),
            "Async function should have yield injected: {output}"
        );
    }

    /// Tests that `inject_item` preserves sync functions.
    #[::core::prelude::v1::test]
    fn inject_item_sync_function() {
        let mut item: Item = syn::parse_quote! {
            fn sync_fn() {
                regular_function();
            }
        };

        let original = quote!(#item).to_string();
        let mut injector = YieldInjector;
        inject_item(&mut item, &mut injector);

        let output = quote!(#item).to_string();
        assert_eq!(output, original, "Sync functions should not be modified");
    }

    /// Tests that `inject_item` processes impl blocks with async methods.
    #[::core::prelude::v1::test]
    fn inject_item_impl_block_async_method() {
        let mut item: Item = syn::parse_quote! {
            impl MyStruct {
                async fn async_method(&self) {
                    self.inner.await;
                }

                fn sync_method(&self) {
                    self.do_something();
                }
            }
        };

        let mut injector = YieldInjector;
        inject_item(&mut item, &mut injector);

        let output = quote!(#item).to_string();
        // Async method should be transformed
        assert!(
            output.contains("yield_now"),
            "Async method in impl should have yield injected: {output}"
        );
        // Sync method should remain unchanged
        assert!(
            output.contains("do_something"),
            "Sync method should be preserved: {output}"
        );
    }

    /// Tests that `inject_item` recursively processes nested modules.
    #[::core::prelude::v1::test]
    fn inject_item_nested_module() {
        let mut item: Item = syn::parse_quote! {
            mod outer {
                async fn outer_fn() {
                    outer_future().await;
                }

                mod inner {
                    async fn inner_fn() {
                        inner_future().await;
                    }
                }
            }
        };

        let mut injector = YieldInjector;
        inject_item(&mut item, &mut injector);

        let output = quote!(#item).to_string();
        // Both outer and inner module async functions should be transformed
        let yield_count = output.matches("yield_now").count();
        assert_eq!(
            yield_count, 2,
            "Both outer and inner async functions should have yields: {output}"
        );
    }

    /// Tests that `inject_item` handles items that don't need transformation.
    #[::core::prelude::v1::test]
    fn inject_item_struct_unchanged() {
        let mut item: Item = syn::parse_quote! {
            struct MyStruct {
                field: i32,
            }
        };

        let original = quote!(#item).to_string();
        let mut injector = YieldInjector;
        inject_item(&mut item, &mut injector);

        let output = quote!(#item).to_string();
        assert_eq!(
            output, original,
            "Struct definitions should not be modified"
        );
    }

    // ============================================================================
    // build_test_tokens tests
    // ============================================================================

    /// Tests that `build_test_tokens` generates correct tokens for "crate" path.
    #[::core::prelude::v1::test]
    fn build_test_tokens_crate_path() {
        let args = TestArgs {
            real_time: false,
            real_fs: false,
            no_simulator: false,
        };
        let item_tokens = quote! { async fn my_test() {} };

        let tokens = build_test_tokens("crate", &args, &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("@ path = crate"),
            "Should contain crate path: {output}"
        );
    }

    /// Tests that `build_test_tokens` generates correct tokens for external crate path.
    #[::core::prelude::v1::test]
    fn build_test_tokens_external_path() {
        let args = TestArgs {
            real_time: false,
            real_fs: false,
            no_simulator: false,
        };
        let item_tokens = quote! { async fn my_test() {} };

        let tokens = build_test_tokens("switchy_async", &args, &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("switchy_async"),
            "Should contain switchy_async path: {output}"
        );
    }

    /// Tests that `build_test_tokens` includes `real_time` flag when set.
    #[::core::prelude::v1::test]
    fn build_test_tokens_with_real_time() {
        let args = TestArgs {
            real_time: true,
            real_fs: false,
            no_simulator: false,
        };
        let item_tokens = quote! { async fn my_test() {} };

        let tokens = build_test_tokens("crate", &args, &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("real_time"),
            "Should contain real_time: {output}"
        );
    }

    /// Tests that `build_test_tokens` includes `real_fs` flag when set.
    #[::core::prelude::v1::test]
    fn build_test_tokens_with_real_fs() {
        let args = TestArgs {
            real_time: false,
            real_fs: true,
            no_simulator: false,
        };
        let item_tokens = quote! { async fn my_test() {} };

        let tokens = build_test_tokens("crate", &args, &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("real_fs"),
            "Should contain real_fs: {output}"
        );
    }

    /// Tests that `build_test_tokens` includes `no_simulator` flag when set.
    #[::core::prelude::v1::test]
    fn build_test_tokens_with_no_simulator() {
        let args = TestArgs {
            real_time: false,
            real_fs: false,
            no_simulator: true,
        };
        let item_tokens = quote! { async fn my_test() {} };

        let tokens = build_test_tokens("crate", &args, &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("no_simulator"),
            "Should contain no_simulator: {output}"
        );
    }

    /// Tests that `build_test_tokens` includes all flags when all are set.
    #[::core::prelude::v1::test]
    fn build_test_tokens_all_flags() {
        let args = TestArgs {
            real_time: true,
            real_fs: true,
            no_simulator: true,
        };
        let item_tokens = quote! { async fn my_test() {} };

        let tokens = build_test_tokens("switchy::unsync", &args, &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("real_time"),
            "Should contain real_time: {output}"
        );
        assert!(
            output.contains("real_fs"),
            "Should contain real_fs: {output}"
        );
        assert!(
            output.contains("no_simulator"),
            "Should contain no_simulator: {output}"
        );
        assert!(output.contains("switchy"), "Should contain path: {output}");
    }

    // ============================================================================
    // MainWithPathInput parsing tests
    // ============================================================================

    /// Tests parsing of `MainWithPathInput` with minimal valid input.
    #[::core::prelude::v1::test]
    fn main_with_path_input_basic() {
        let input = quote! {
            @path = crate;
            async fn main() {}
        };
        let parsed: MainWithPathInput =
            syn::parse2(input).expect("Failed to parse basic MainWithPathInput");

        assert_eq!(
            parsed.crate_path.segments.last().unwrap().ident.to_string(),
            "crate"
        );
        assert_eq!(parsed.function.sig.ident.to_string(), "main");
    }

    /// Tests parsing of `MainWithPathInput` with external crate path.
    #[::core::prelude::v1::test]
    fn main_with_path_input_external_path() {
        let input = quote! {
            @path = switchy_async;
            async fn main() {}
        };
        let parsed: MainWithPathInput =
            syn::parse2(input).expect("Failed to parse MainWithPathInput with external path");

        assert_eq!(
            parsed.crate_path.segments.last().unwrap().ident.to_string(),
            "switchy_async"
        );
    }

    /// Tests parsing of `MainWithPathInput` with qualified crate path.
    #[::core::prelude::v1::test]
    fn main_with_path_input_qualified_path() {
        let input = quote! {
            @path = switchy::unsync;
            async fn main() {}
        };
        let parsed: MainWithPathInput =
            syn::parse2(input).expect("Failed to parse MainWithPathInput with qualified path");

        assert_eq!(parsed.crate_path.segments.len(), 2);
        assert_eq!(parsed.crate_path.segments[0].ident.to_string(), "switchy");
        assert_eq!(parsed.crate_path.segments[1].ident.to_string(), "unsync");
    }

    /// Tests parsing of `MainWithPathInput` with return type.
    #[::core::prelude::v1::test]
    fn main_with_path_input_with_return_type() {
        let input = quote! {
            @path = crate;
            async fn main() -> Result<(), Box<dyn std::error::Error>> {}
        };
        let parsed: MainWithPathInput =
            syn::parse2(input).expect("Failed to parse MainWithPathInput with return type");

        assert!(
            matches!(parsed.function.sig.output, syn::ReturnType::Type(_, _)),
            "Should have a return type"
        );
    }

    // ============================================================================
    // build_main_tokens tests
    // ============================================================================

    /// Tests that `build_main_tokens` generates correct tokens for "crate" path.
    #[::core::prelude::v1::test]
    fn build_main_tokens_crate_path() {
        let item_tokens = quote! { async fn main() {} };

        let tokens = build_main_tokens("crate", &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("@ path = crate"),
            "Should contain crate path: {output}"
        );
    }

    /// Tests that `build_main_tokens` generates correct tokens for external crate path.
    #[::core::prelude::v1::test]
    fn build_main_tokens_external_path() {
        let item_tokens = quote! { async fn main() {} };

        let tokens = build_main_tokens("switchy_async", &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("switchy_async"),
            "Should contain switchy_async path: {output}"
        );
    }

    /// Tests that `build_main_tokens` generates correct tokens for qualified path.
    #[::core::prelude::v1::test]
    fn build_main_tokens_qualified_path() {
        let item_tokens = quote! { async fn main() {} };

        let tokens = build_main_tokens("switchy::unsync", &item_tokens);
        let output = tokens.to_string();

        assert!(
            output.contains("switchy"),
            "Should contain switchy: {output}"
        );
        assert!(output.contains("unsync"), "Should contain unsync: {output}");
    }
}
