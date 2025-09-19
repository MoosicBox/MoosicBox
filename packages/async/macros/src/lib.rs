#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::PathBuf;
use std::str::FromStr as _;

use proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::visit_mut::{VisitMut, visit_expr_mut};
use syn::{Expr, ImplItem, Item, ItemMod, parse_macro_input};

#[cfg(feature = "simulator")]
mod simulator;

/// Represents the parsed input to a test macro with crate path
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

/// Represents the parsed arguments for test attributes
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

/// Helper function to convert `TestArgs` to the internal token format
#[cfg(feature = "simulator")]
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

/// Internal select! macro that accepts a crate path parameter
/// This provides 100% `tokio::select`! compatibility while automatically
/// fusing futures/streams for the simulator runtime
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn select_internal(input: TokenStream) -> TokenStream {
    simulator::select_internal(input)
}

/// Internal join! macro that accepts a crate path parameter
/// This provides 100% `tokio::join!` compatibility for the simulator runtime
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn join_internal(input: TokenStream) -> TokenStream {
    simulator::join_internal(input)
}

/// Internal `try_join`! macro that accepts a crate path parameter
/// This provides 100% `tokio::try_join!` compatibility for the simulator runtime
#[cfg(feature = "simulator")]
#[proc_macro]
pub fn try_join_internal(input: TokenStream) -> TokenStream {
    simulator::try_join_internal(input)
}

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

/// Internal test attribute macro that accepts a crate path parameter
/// This provides test runtime setup for the simulator runtime
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

/// Internal test attribute macro for `switchy_async` - uses crate path for internal usage
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

/// External test attribute macro for `switchy_async` - uses `switchy_async` path for external usage
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

/// Test attribute macro for `switchy::unsync` - accepts optional `real_time` parameter
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

/// Tokio-compatible test attribute macro - accepts `real_time` parameter but treats it as no-op
#[proc_macro_attribute]
pub fn tokio_test_wrapper(args: TokenStream, item: TokenStream) -> TokenStream {
    // Parse and ignore the real_time parameter - tokio doesn't support it
    let _args_str = if args.is_empty() {
        String::new()
    } else {
        args.to_string() // Parse but ignore
    };

    // Always generate a standard tokio::test regardless of parameters
    let item_tokens: TokenStream2 = item.into();
    let result = quote! {
        #[::tokio::test]
        #item_tokens
    };

    result.into()
}

/// # Panics
///
/// * If fails to get the `CARGO_MANIFEST_DIR` environment variable
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
