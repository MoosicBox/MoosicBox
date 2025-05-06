#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::PathBuf;
use std::str::FromStr as _;

use proc_macro::TokenStream;
use quote::quote;
use syn::visit_mut::{VisitMut, visit_expr_mut};
use syn::{Expr, ImplItem, Item, ItemMod, parse_macro_input};

struct YieldInjector;

impl VisitMut for YieldInjector {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        visit_expr_mut(self, expr);

        if let Expr::Await(expr_await) = expr {
            let base = (*expr_await.base).clone();
            *expr = syn::parse_quote!({
                let __yield_res = #base.await;
                ::moosicbox_async::task::yield_now().await;
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
                if let ImplItem::Fn(func) = impl_member {
                    if func.sig.asyncness.is_some() {
                        injector.visit_block_mut(&mut func.block);
                    }
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
