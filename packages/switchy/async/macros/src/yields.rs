//! Yield injection: rewriting every `.await` an item contains, and loading the files of a
//! module for `inject_yields_mod!`.

use std::path::{Path, PathBuf};

use proc_macro2::TokenStream as TokenStream2;
use syn::{
    Attribute, Expr, ImplItem, Item, ItemMod, Lit, Meta, TraitItem,
    spanned::Spanned as _,
    visit_mut::{self, VisitMut},
};

/// Whether `attrs` hold `#[inject_yields]` (by any path ending in `inject_yields`).
pub fn has_inject_yields(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "inject_yields")
    })
}

/// Rewrites every `.await` it visits to `<path>::task::yield_after(base).await`, which awaits
/// `base` and then yields with the simulation run's probability.
///
/// It reaches every `.await` the parser sees: in `async fn` bodies, `async` blocks and
/// closures (in any function, sync or async), impl and trait items, and nested modules.
/// Awaits inside macro invocations are opaque tokens and stay as they are. Items carrying
/// their own `#[inject_yields]` are left to that attribute, so nothing yields twice.
///
/// The rewrite is a plain call expression, so it fits wherever the `.await` did (`?`,
/// method chains, `match` scrutinees, `let ... else`, `while let` heads) and keeps the
/// original's temporaries and drop order.
pub struct YieldInjector {
    pub yield_path: TokenStream2,
}

impl YieldInjector {
    /// Rewrite every `.await` in `item` (whose own `#[inject_yields]` is already consumed).
    pub fn inject_item(&mut self, item: &mut Item) {
        visit_mut::visit_item_mut(self, item);
    }
}

impl VisitMut for YieldInjector {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        visit_mut::visit_expr_mut(self, expr);

        if let Expr::Await(expr_await) = expr {
            let attrs = std::mem::take(&mut expr_await.attrs);
            let base = &expr_await.base;
            let yield_path = &self.yield_path;
            let span = expr_await.await_token.span();
            let mut rewritten: Expr = syn::parse_quote_spanned! {span=>
                #yield_path::task::yield_after(#base).await
            };
            if let Expr::Await(rewritten) = &mut rewritten {
                rewritten.attrs = attrs;
            }
            *expr = rewritten;
        }
    }

    fn visit_item_mut(&mut self, item: &mut Item) {
        let attrs = match item {
            Item::Fn(item) => &item.attrs,
            Item::Impl(item) => &item.attrs,
            Item::Mod(item) => &item.attrs,
            Item::Trait(item) => &item.attrs,
            Item::Const(item) => &item.attrs,
            Item::Static(item) => &item.attrs,
            _ => return visit_mut::visit_item_mut(self, item),
        };
        if !has_inject_yields(attrs) {
            visit_mut::visit_item_mut(self, item);
        }
    }

    fn visit_impl_item_mut(&mut self, item: &mut ImplItem) {
        let own = match item {
            ImplItem::Fn(item) => has_inject_yields(&item.attrs),
            ImplItem::Const(item) => has_inject_yields(&item.attrs),
            _ => false,
        };
        if !own {
            visit_mut::visit_impl_item_mut(self, item);
        }
    }

    fn visit_trait_item_mut(&mut self, item: &mut TraitItem) {
        let own = match item {
            TraitItem::Fn(item) => has_inject_yields(&item.attrs),
            TraitItem::Const(item) => has_inject_yields(&item.attrs),
            _ => false,
        };
        if !own {
            visit_mut::visit_trait_item_mut(self, item);
        }
    }
}

/// Where a module's out-of-line children live.
#[derive(Clone)]
struct Dirs {
    /// `mod child;` loads `<children>/child.rs` or `<children>/child/mod.rs`.
    children: PathBuf,
    /// `#[path = "p"] mod child;` loads `<path_base>/p`.
    path_base: PathBuf,
}

impl Dirs {
    /// The directories of the module whose file is `file`.
    fn of_file(file: &Path, mod_rs: bool) -> Self {
        let parent = file.parent().map_or_else(PathBuf::new, Path::to_path_buf);
        let children = if mod_rs {
            parent.clone()
        } else {
            parent.join(file.file_stem().unwrap_or_default())
        };
        Self {
            children,
            path_base: parent,
        }
    }

    /// The directories of an inline module `name` declared in this one.
    fn inline(&self, name: &str) -> Self {
        let dir = self.children.join(name);
        Self {
            children: dir.clone(),
            path_base: dir,
        }
    }
}

/// Whether `file` is a "mod-rs" file: a crate root or a `mod.rs`, whose child modules sit
/// beside it.
fn is_mod_rs(file: &Path) -> bool {
    matches!(
        file.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "main.rs" | "mod.rs")
    )
}

/// The `#[path = "..."]` attribute's value.
fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| match &attr.meta {
        Meta::NameValue(meta) if meta.path.is_ident("path") => match &meta.value {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Str(value) => Some(value.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })
}

/// Loads the files of out-of-line modules into the module tree, as rustc would find them.
pub struct ModuleLoader {
    /// Every file loaded, for the expansion to `include_str!` so that editing one rebuilds.
    pub files: Vec<PathBuf>,
}

impl ModuleLoader {
    pub const fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Load `module`, declared in `file`, and every out-of-line module inside it. A module
    /// whose file does not exist stays a declaration (rustc reports it if it is not
    /// configured out).
    ///
    /// # Errors
    ///
    /// * If `module` is a declaration whose file is missing, or a file cannot be parsed
    pub fn load_declared_in(&mut self, module: &mut ItemMod, file: &Path) -> Result<(), String> {
        // A crate root not named `lib.rs` or `main.rs` (an integration test, an example, a
        // binary in `src/bin`) is a mod-rs file too, which its name cannot tell.
        let mod_rs = is_mod_rs(file);
        let mut layouts = vec![Dirs::of_file(file, mod_rs)];
        if !mod_rs {
            layouts.push(Dirs::of_file(file, true));
        }
        let mut loaded = false;
        for dirs in &layouts {
            if self.load_module(module, dirs)? {
                loaded = true;
                break;
            }
        }
        if !loaded {
            return Err(format!(
                "inject_yields_mod!: no file for module `{}` declared in {}",
                module.ident,
                file.display()
            ));
        }
        Ok(())
    }

    /// Load an out-of-line `module` (declared in a module with `dirs`) or recurse into an
    /// inline one. Returns whether the module now has its content.
    fn load_module(&mut self, module: &mut ItemMod, dirs: &Dirs) -> Result<bool, String> {
        let name = module.ident.to_string();
        if let Some((_, items)) = &mut module.content {
            self.load_items(items, &dirs.inline(&name))?;
            return Ok(true);
        }
        let candidates = path_attr(&module.attrs).map_or_else(
            || {
                vec![
                    (dirs.children.join(format!("{name}.rs")), false),
                    (dirs.children.join(&name).join("mod.rs"), true),
                ]
            },
            |path| vec![(dirs.path_base.join(path), true)],
        );
        let Some((file, mod_rs)) = candidates.into_iter().find(|(file, _)| file.is_file()) else {
            return Ok(false);
        };
        let code = std::fs::read_to_string(&file)
            .map_err(|e| format!("inject_yields_mod!: reading {}: {e}", file.display()))?;
        let parsed = syn::parse_file(&code)
            .map_err(|e| format!("inject_yields_mod!: parsing {}: {e}", file.display()))?;
        let mut items = parsed.items;
        // A module file named by `#[path]` places its children as a mod-rs file does.
        let file_dirs = Dirs::of_file(&file, mod_rs);
        self.load_items(&mut items, &file_dirs)?;
        self.files
            .push(std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone()));

        module.attrs.retain(|attr| !attr.path().is_ident("path"));
        module.attrs.extend(parsed.attrs);
        module.content = Some((syn::token::Brace(module.span()), items));
        module.semi = None;
        Ok(true)
    }

    fn load_items(&mut self, items: &mut [Item], dirs: &Dirs) -> Result<(), String> {
        for item in items {
            if let Item::Mod(module) = item {
                self.load_module(module, dirs)?;
            }
        }
        Ok(())
    }
}

/// `const _: &str = include_str!("<file>");` for each file, so that editing a loaded module
/// file rebuilds the crate (rustc itself never opens it).
pub fn include_files(files: &[PathBuf]) -> Vec<Item> {
    files
        .iter()
        .map(|file| {
            let file = file.display().to_string();
            let span = proc_macro2::Span::call_site();
            syn::parse_quote_spanned! {span=>
                const _: &str = ::core::include_str!(#file);
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use quote::{ToTokens as _, quote};

    use super::*;

    fn inject(item: &mut Item) -> String {
        let mut injector = YieldInjector {
            yield_path: quote!(::switchy_async),
        };
        injector.inject_item(item);
        item.to_token_stream().to_string()
    }

    fn yields(output: &str) -> usize {
        output.matches("yield_after").count()
    }

    #[::core::prelude::v1::test]
    fn rewrites_an_await_to_a_call() {
        let mut item: Item = syn::parse_quote! {
            async fn f() { some_future().await; }
        };
        let output = inject(&mut item);
        assert!(
            output.contains(":: switchy_async :: task :: yield_after (some_future ()) . await"),
            "{output}"
        );
    }

    #[::core::prelude::v1::test]
    fn reaches_awaits_in_sync_fns_async_blocks_and_closures() {
        let mut item: Item = syn::parse_quote! {
            fn f() {
                spawn(async move { a.await });
                let c = async |x| x.await;
                let d = || async { b.await };
            }
        };
        assert_eq!(yields(&inject(&mut item)), 3);
    }

    #[::core::prelude::v1::test]
    fn reaches_nested_awaits_and_chains() {
        let mut item: Item = syn::parse_quote! {
            async fn f() -> Result<(), E> {
                g(h().await).await;
                a.await.b().await?.c();
                let Some(x) = i().await else { return Ok(()) };
                match j().await { _ => {} }
                while let Some(y) = k().await {}
                loop { if l().await { break } }
                Ok(())
            }
        };
        let output = inject(&mut item);
        assert_eq!(yields(&output), 8, "{output}");
        // `let ... else` takes a plain call expression, which no `}` ends.
        assert!(
            output.contains(
                "let Some (x) = :: switchy_async :: task :: yield_after (i ()) . await else"
            ),
            "{output}"
        );
    }

    #[::core::prelude::v1::test]
    fn reaches_impls_traits_and_inline_modules() {
        let mut item: Item = syn::parse_quote! {
            mod m {
                impl Trait for S {
                    async fn a(&self) { x.await }
                    fn b(&self) -> impl Future { async { y.await } }
                }
                trait T {
                    async fn c(&self) { z.await }
                }
                mod inner {
                    async fn d() { w.await }
                }
            }
        };
        assert_eq!(yields(&inject(&mut item)), 4);
    }

    #[::core::prelude::v1::test]
    fn leaves_items_with_their_own_attribute() {
        let mut item: Item = syn::parse_quote! {
            mod m {
                #[inject_yields]
                async fn a() { x.await }
                #[switchy_async::inject_yields]
                impl S {
                    async fn b(&self) { y.await }
                }
                impl S {
                    #[inject_yields]
                    async fn c(&self) { z.await }
                    async fn d(&self) { w.await }
                }
            }
        };
        assert_eq!(yields(&inject(&mut item)), 1);
    }

    #[::core::prelude::v1::test]
    fn leaves_awaits_inside_macro_invocations() {
        let mut item: Item = syn::parse_quote! {
            async fn f() { println!("{}", x.await); }
        };
        assert_eq!(yields(&inject(&mut item)), 0);
    }

    #[::core::prelude::v1::test]
    fn leaves_code_without_awaits() {
        let mut item: Item = syn::parse_quote! {
            impl S { fn f(&self) -> i32 { 1 + g() } }
        };
        let original = item.to_token_stream().to_string();
        assert_eq!(inject(&mut item), original);
    }

    /// A directory of its own under the system's temporary directory.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "switchy_async_macros_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, code: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, code).unwrap();
    }

    #[::core::prelude::v1::test]
    fn loads_module_files_as_rustc_does() {
        let dir = scratch("loads");
        let src = dir.join("src");
        write(&src.join("lib.rs"), "");
        write(
            &src.join("runtime.rs"),
            "//! Runtime.\n#![allow(dead_code)]\nmod a;\nmod b;\n#[path = \"other.rs\"] mod c;\nmod inline { mod d; }\n#[cfg(any())] mod missing;\n",
        );
        write(&src.join("runtime/a.rs"), "async fn a() { x.await }");
        write(&src.join("runtime/b/mod.rs"), "mod e;");
        write(&src.join("runtime/b/e.rs"), "async fn e() { x.await }");
        write(&src.join("other.rs"), "async fn c() { x.await }");
        write(&src.join("runtime/inline/d.rs"), "async fn d() { x.await }");

        let mut module: ItemMod = syn::parse_quote!(
            pub(crate) mod runtime;
        );
        let mut loader = ModuleLoader::new();
        loader
            .load_declared_in(&mut module, &src.join("lib.rs"))
            .unwrap();
        assert_eq!(loader.files.len(), 6, "{:?}", loader.files);

        let mut item = Item::Mod(module);
        let output = inject(&mut item);
        assert_eq!(yields(&output), 4, "{output}");
        // The file's inner attributes stay inside the module.
        assert!(
            output.starts_with(
                "pub (crate) mod runtime { # ! [doc = \" Runtime.\"] # ! [allow (dead_code)]"
            ),
            "{output}"
        );
        assert!(!output.contains("path ="), "{output}");
        assert!(output.contains("mod missing ;"), "{output}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[::core::prelude::v1::test]
    fn loads_from_a_crate_root_with_another_name() {
        let dir = scratch("roots");
        write(&dir.join("tests/it.rs"), "");
        write(&dir.join("tests/helpers/mod.rs"), "mod inner;");
        write(
            &dir.join("tests/helpers/inner.rs"),
            "async fn f() { x.await }",
        );

        let mut module: ItemMod = syn::parse_quote!(
            mod helpers;
        );
        let mut loader = ModuleLoader::new();
        loader
            .load_declared_in(&mut module, &dir.join("tests/it.rs"))
            .unwrap();
        assert_eq!(yields(&inject(&mut Item::Mod(module))), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[::core::prelude::v1::test]
    fn reports_a_missing_module_file() {
        let dir = scratch("missing");
        let mut module: ItemMod = syn::parse_quote!(
            mod absent;
        );
        let error = ModuleLoader::new()
            .load_declared_in(&mut module, &dir.join("src/lib.rs"))
            .unwrap_err();
        assert!(error.contains("absent"), "{error}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
